use pipedash_plugin_api::{
    LogCursor,
    LogNode,
    LogRead,
    LogSink,
    PluginError,
    PluginResult,
    RunRef,
};

use super::ArgocdPlugin;
use crate::client::PodLogRequest;
use crate::{
    config,
    logs,
    mapper,
};

impl ArgocdPlugin {
    async fn read_pod_log(
        &self, app_namespace: &str, app_name: &str, namespace: &str, pod_name: &str,
        sink: &mut LogSink,
    ) -> PluginResult<()> {
        let client = self.client()?;
        let pod = client
            .pod(app_namespace, app_name, namespace, pod_name)
            .await?;
        let events = client
            .pod_events(app_name, namespace, pod_name, &pod.metadata.uid)
            .await?;
        let mut out = Vec::new();
        logs::pods::write_pod_summary(&pod, namespace, pod_name, events, &mut out);
        sink.write(out.into()).await?;

        for container in logs::pods::container_logs(&pod) {
            let mut header = Vec::new();
            container.write_header(&mut header);
            sink.write(header.into()).await?;
            let request = PodLogRequest {
                namespace,
                pod: pod_name,
                container: container.name,
                previous: container.previous,
            };
            if client.stream_pod_log(app_name, request, sink).await? == 0 {
                let mut empty = Vec::new();
                logs::pods::write_no_output(&mut empty);
                sink.write(empty.into()).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn fetch_log_nodes_impl(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (namespace, app_name) = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let app = client.get_application(&namespace, &app_name).await?;
        let sync = logs::find_sync(&app, run.run_number).ok_or_else(|| {
            PluginError::PipelineNotFound(format!("Run {} not found", run.run_number))
        })?;
        let tree = client.resource_tree(&app).await?;

        Ok(logs::log_nodes(
            &sync,
            &tree,
            &mapper::app_url(self.get_server_url(), &app_name),
        ))
    }

    pub(super) async fn read_log_impl(
        &self, run: RunRef<'_>, node_id: &str, _from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (app_namespace, app_name) = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let mut out = Vec::new();

        if node_id == logs::SYNC_NODE {
            let app = client.get_application(&app_namespace, &app_name).await?;
            let sync = logs::find_sync(&app, run.run_number).ok_or_else(|| {
                PluginError::PipelineNotFound(format!("Run {} not found", run.run_number))
            })?;
            logs::sync::write_sync(&sync, &mut out);
        } else if node_id == logs::EVENTS_NODE {
            logs::write_events(
                client.application_events(&app_namespace, &app_name).await?,
                &mut out,
            );
        } else {
            let (namespace, pod_name) = logs::pod_of_node(node_id).ok_or_else(|| {
                PluginError::InvalidConfig(format!("Invalid ArgoCD log node: {node_id}"))
            })?;
            return self
                .read_pod_log(&app_namespace, &app_name, namespace, pod_name, sink)
                .await
                .map(|()| LogRead::replace(None));
        }
        sink.write(out.into())
            .await
            .map(|()| LogRead::replace(None))
    }
}
