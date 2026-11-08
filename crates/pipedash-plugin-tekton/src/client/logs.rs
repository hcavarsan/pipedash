use std::time::Duration;

use futures::AsyncBufReadExt;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::jiff::Timestamp;
use kube::api::{
    Api,
    LogParams,
};
use pipedash_plugin_api::kubernetes::map_error;
use pipedash_plugin_api::{
    LogSink,
    PluginError,
    PluginResult,
    LOG_CHUNK_STALL_TIMEOUT,
};

use super::{
    TektonClient,
    STEP_LOG_TAIL_LINES,
};
use crate::log_dialect;
use crate::step_log::{
    ContainerCursor,
    StepFilter,
    StepSummary,
};

const STEP_LOG_TIMEOUT: Duration = Duration::from_secs(900);

impl TektonClient {
    pub async fn get_pod(&self, namespace: &str, pod: &str) -> PluginResult<Pod> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        self.retry_policy
            .retry(|| async {
                pods.get(pod)
                    .await
                    .map_err(|error| map_pod_error(error, pod, "Failed to read the pod"))
            })
            .await
    }

    pub async fn stream_step_log(
        &self, namespace: &str, pod: &str, container: &str, prior: Option<&ContainerCursor>,
        hold_tail: bool, sink: &mut LogSink,
    ) -> PluginResult<Option<StepSummary>> {
        tokio::time::timeout(
            STEP_LOG_TIMEOUT,
            self.pipe_step_log(namespace, pod, container, prior, hold_tail, sink),
        )
        .await
        .map_err(|_| {
            PluginError::NetworkError("Pod log download timed out after 900 seconds".into())
        })?
    }

    async fn pipe_step_log(
        &self, namespace: &str, pod: &str, container: &str, prior: Option<&ContainerCursor>,
        hold_tail: bool, sink: &mut LogSink,
    ) -> PluginResult<Option<StepSummary>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let from = prior.and_then(|prior| prior.last);
        let params = LogParams {
            container: Some(container.to_string()),
            timestamps: true,
            tail_lines: prior.is_none().then_some(STEP_LOG_TAIL_LINES as i64),
            since_time: from.and_then(|stamp| Timestamp::from_second(stamp.at.timestamp()).ok()),
            ..LogParams::default()
        };
        let stream = self
            .retry_policy
            .retry(|| async {
                pods.log_stream(pod, &params)
                    .await
                    .map_err(|error| map_pod_error(error, pod, "Failed to read pod logs"))
            })
            .await?;
        let mut stream = std::pin::pin!(stream);

        let mut filter = StepFilter::new(from, hold_tail);
        let mut out = Vec::new();
        loop {
            let chunk = match tokio::time::timeout(LOG_CHUNK_STALL_TIMEOUT, stream.fill_buf()).await
            {
                Err(_) => {
                    return Err(PluginError::NetworkError(
                        "Log download stalled for 60 seconds".into(),
                    ))
                }
                Ok(Err(e)) => {
                    return Err(PluginError::NetworkError(format!(
                        "Failed to read pod logs: {e}"
                    )))
                }
                Ok(Ok([])) => break,
                Ok(Ok(chunk)) => chunk,
            };
            let read = chunk.len();
            let pushed = filter.push(&log_dialect::without_line_tags(chunk), &mut out);
            stream.consume_unpin(read);
            if pushed.is_err() {
                return Ok(None);
            }
            sink.write(std::mem::take(&mut out).into()).await?;
        }
        let Ok(summary) = filter.finish(&mut out) else {
            return Ok(None);
        };
        sink.write(out.into()).await?;
        Ok(Some(summary))
    }
}

fn map_pod_error(error: kube::Error, pod: &str, context: &str) -> PluginError {
    match error {
        kube::Error::Api(status) if status.code == 404 => {
            PluginError::PipelineNotFound(format!("The pod {pod} of this TaskRun no longer exists"))
        }
        other => map_error(other, context),
    }
}
