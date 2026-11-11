use futures::future::try_join;
use pipedash_plugin_api::{
    LogCursor,
    LogNode,
    LogRead,
    LogSink,
    PluginError,
    PluginResult,
    RunRef,
};

use super::TektonPlugin;
use crate::step_log::{
    self,
    ContainerCursor,
    StartedStep,
    TektonCursor,
};
use crate::{
    client,
    config,
    log_dialect,
    mapper,
    types,
};

pub(super) fn find_run(
    runs: Vec<types::TektonPipelineRun>, run_number: i64,
) -> PluginResult<types::TektonPipelineRun> {
    let mut matching: Vec<_> = runs
        .into_iter()
        .filter(|r| mapper::run_number(&r.metadata) == run_number)
        .collect();

    match matching.len() {
        0 => Err(PluginError::PipelineNotFound(format!(
            "PipelineRun #{run_number} not found"
        ))),
        1 => Ok(matching.remove(0)),
        _ => {
            let names: Vec<_> = matching.iter().map(|r| r.metadata.name.as_str()).collect();
            Err(PluginError::ApiError(format!(
                "PipelineRuns {} share run number {run_number}. Use kubectl to pick one",
                names.join(", ")
            )))
        }
    }
}

fn run_target<'a>(run: &RunRef<'a>) -> PluginResult<(&'a str, &'a str)> {
    let metadata = run.metadata;
    let field = |key: &str| {
        metadata
            .get(key)
            .and_then(|value| value.as_str())
            .filter(|value| config::is_kube_name(value))
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("Run {} has no valid {key}", run.run_id))
            })
    };
    Ok((field("namespace")?, field("pipelinerun_name")?))
}

async fn pipe_steps(
    client: &client::TektonClient, namespace: &str, pod: &str, uid: &str,
    steps: &[StartedStep<'_>], prior: Option<&TektonCursor>, sink: &mut LogSink,
) -> PluginResult<Option<TektonCursor>> {
    let mut containers = Vec::with_capacity(steps.len());
    for (index, started) in steps.iter().enumerate() {
        let before = prior.and_then(|prior| prior.containers.get(index));
        if let Some(before) = before.filter(|before| before.done) {
            containers.push(before.clone());
            continue;
        }
        if before.is_none() {
            let header = if started.sidecar {
                log_dialect::sidecar_header(&started.step.name, started.step.started_at())
            } else {
                log_dialect::step_header(&started.step.name, started.step.started_at())
            };
            sink.write(header.into()).await?;
        }
        let Some(read) = client
            .stream_step_log(
                namespace,
                pod,
                &started.container,
                before,
                !started.done,
                sink,
            )
            .await?
        else {
            return Ok(None);
        };
        let capped = before.is_none() && read.lines >= client::STEP_LOG_TAIL_LINES;
        if capped {
            sink.write(
                log_dialect::truncation_notice(client::STEP_LOG_TAIL_LINES, started.sidecar).into(),
            )
            .await?;
        }
        containers.push(ContainerCursor {
            name: started.container.clone(),
            instance: started.instance.clone(),
            done: started.done,
            capped,
            last: read.last,
        });
    }
    Ok(Some(TektonCursor {
        pod: uid.to_string(),
        containers,
    }))
}

impl TektonPlugin {
    pub(super) async fn fetch_log_nodes_impl(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (namespace, pipelinerun_name) = run_target(&run)?;
        let client = self.client().await?;

        let (pipelinerun, task_runs) = try_join(
            client.get_pipelinerun(namespace, pipelinerun_name),
            client.list_taskruns(namespace, pipelinerun_name),
        )
        .await?;

        Ok(mapper::logs::taskruns_to_log_nodes(
            &pipelinerun,
            &task_runs,
        ))
    }

    pub(super) async fn read_log_impl(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (namespace, pipelinerun_name) = run_target(&run)?;
        let (task_name, sidecar_name) = node_id
            .split_once(":sidecar:")
            .map_or((node_id, None), |(task, sidecar)| (task, Some(sidecar)));
        if !config::is_kube_name(task_name)
            || sidecar_name.is_some_and(|name| !config::is_kube_name(name))
        {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid Tekton log node: {node_id}"
            )));
        }
        let client = self.client().await?;

        let pipelinerun = client.get_pipelinerun(namespace, pipelinerun_name).await?;
        if !mapper::logs::listed_taskruns(&pipelinerun)
            .iter()
            .any(|(name, _)| *name == task_name)
        {
            return Err(PluginError::PipelineNotFound(format!(
                "PipelineRun {pipelinerun_name} has no TaskRun {task_name}"
            )));
        }

        let task_run = client.get_taskrun(namespace, task_name).await?;
        let selected = match sidecar_name {
            Some(name) => {
                let sidecar = task_run
                    .status
                    .sidecars
                    .iter()
                    .find(|sidecar| sidecar.name == name)
                    .ok_or_else(|| {
                        PluginError::PipelineNotFound(format!(
                            "TaskRun {task_name} has no sidecar {name}"
                        ))
                    })?;
                std::slice::from_ref(sidecar)
            }
            None => task_run.status.steps.as_slice(),
        };
        let pod = task_run
            .status
            .pod_name
            .as_deref()
            .filter(|pod| !pod.is_empty())
            .ok_or_else(|| {
                let reason = task_run
                    .status
                    .conditions
                    .iter()
                    .find(|condition| condition.type_ == "Succeeded")
                    .map(|condition| condition.message.as_str())
                    .filter(|message| !message.is_empty());
                PluginError::PipelineNotFound(match reason {
                    Some(reason) => format!("TaskRun {node_id} has no pod: {reason}"),
                    None => format!("TaskRun {node_id} has no pod"),
                })
            })?;
        if !config::is_kube_name(pod) {
            return Err(PluginError::ApiError(format!(
                "TaskRun {node_id} reports an invalid pod name: {pod}"
            )));
        }

        let pod_object = client.get_pod(namespace, pod).await?;
        let uid = pod_object.metadata.uid.as_deref().unwrap_or_default();
        let steps = step_log::started_steps(selected, &pod_object, sidecar_name.is_some());

        let prior = from
            .and_then(|from| from.parse::<TektonCursor>())
            .filter(|prior| prior.continues(uid, &steps));
        if let Some(prior) = &prior {
            if let Some(next) =
                pipe_steps(client, namespace, pod, uid, &steps, Some(prior), sink).await?
            {
                return Ok(LogRead::append(LogCursor::new(&next)?));
            }
        }
        if from.is_some() {
            sink.restart()?;
        }
        let next = pipe_steps(client, namespace, pod, uid, &steps, None, sink)
            .await?
            .ok_or_else(|| PluginError::Internal("A full Tekton log read diverged".into()))?;
        let cursor = if next.snapshot_only() {
            None
        } else {
            Some(LogCursor::new(&next)?)
        };
        Ok(LogRead::replace(cursor))
    }
}
