use pipedash_plugin_api::{
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
};

use super::map_status;
use crate::types::{
    self,
    StepState,
    TektonPipelineRun,
    TektonTaskRun,
};

pub(crate) fn listed_taskruns(run: &TektonPipelineRun) -> Vec<(&str, &str)> {
    let children: Vec<_> = run
        .status
        .child_references
        .iter()
        .filter(|child| child.kind == "TaskRun")
        .map(|child| (child.name.as_str(), child.pipeline_task_name.as_str()))
        .collect();
    if !children.is_empty() {
        return children;
    }

    let mut legacy: Vec<_> = run
        .status
        .task_runs
        .iter()
        .map(|(name, task_run)| (name.as_str(), task_run.pipeline_task_name.as_str()))
        .collect();
    legacy.sort_unstable();
    legacy
}

fn log_availability(status: PipelineStatus) -> LogAvailability {
    match status {
        PipelineStatus::Running => LogAvailability::Partial,
        PipelineStatus::Pending => LogAvailability::Pending,
        _ => LogAvailability::Ready,
    }
}

fn step_status(step: &StepState) -> PipelineStatus {
    match (&step.terminated, &step.running) {
        (Some(_), _) if step.termination_reason.as_deref() == Some("Skipped") => {
            PipelineStatus::Skipped
        }
        (Some(terminated), _) if terminated.exit_code == 0 => PipelineStatus::Success,
        (Some(_), _) => PipelineStatus::Failed,
        (None, Some(_)) => PipelineStatus::Running,
        (None, None) => PipelineStatus::Pending,
    }
}

fn step_to_log_node(task_run: &str, step: &StepState, sidecar: bool) -> LogNode {
    let status = step_status(step);
    LogNode {
        id: if sidecar {
            format!("{task_run}:sidecar:{}", step.name)
        } else {
            format!("{task_run}:{}", step.name)
        },
        parent_id: Some(task_run.to_string()),
        kind: LogNodeKind::Step,
        name: if sidecar {
            format!("Sidecar: {}", step.name)
        } else {
            step.name.clone()
        },
        status,
        started_at: step.started_at(),
        finished_at: step.finished_at(),
        log: sidecar.then(|| log_availability(status)),
        web_url: None,
    }
}

pub(crate) fn taskruns_to_log_nodes(
    run: &TektonPipelineRun, task_runs: &[TektonTaskRun],
) -> Vec<LogNode> {
    let mut nodes = Vec::new();
    for (name, pipeline_task) in listed_taskruns(run) {
        let Some(task_run) = task_runs
            .iter()
            .find(|task_run| task_run.metadata.name == name)
        else {
            continue;
        };
        let status = map_status(&task_run.status.conditions);

        nodes.push(LogNode {
            id: name.to_string(),
            parent_id: None,
            kind: LogNodeKind::Job,
            name: if pipeline_task.is_empty() {
                name.to_string()
            } else {
                pipeline_task.to_string()
            },
            status,
            started_at: types::parse_timestamp(&task_run.status.start_time),
            finished_at: types::parse_timestamp(&task_run.status.completion_time),
            log: Some(log_availability(status)),
            web_url: None,
        });
        nodes.extend(
            task_run
                .status
                .steps
                .iter()
                .map(|step| step_to_log_node(name, step, false)),
        );
        nodes.extend(
            task_run
                .status
                .sidecars
                .iter()
                .map(|sidecar| step_to_log_node(name, sidecar, true)),
        );
    }
    nodes
}
