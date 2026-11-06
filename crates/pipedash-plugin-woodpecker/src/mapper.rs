use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};

use crate::{
    config,
    types,
};

pub(crate) const BRANCH_INPUT: &str = "branch";

pub(crate) fn map_status(status: &str) -> PipelineStatus {
    match status {
        "success" => PipelineStatus::Success,
        "failure" | "error" => PipelineStatus::Failed,
        "killed" | "canceled" | "declined" => PipelineStatus::Cancelled,
        "skipped" => PipelineStatus::Skipped,
        "running" => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

fn timestamp(seconds: i64) -> Option<DateTime<Utc>> {
    (seconds > 0)
        .then(|| DateTime::from_timestamp(seconds, 0))
        .flatten()
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

pub(crate) fn map_available_pipeline(repo: &types::Repo) -> AvailablePipeline {
    AvailablePipeline {
        id: repo.full_name.clone(),
        name: repo.full_name.clone(),
        description: None,
        organization: Some(repo.owner.clone()),
        repository: Some(repo.name.clone()),
    }
}

pub(crate) fn map_pipeline(
    provider_id: i64, repo: &types::Repo, latest: Option<&types::Pipeline>,
) -> Pipeline {
    Pipeline {
        id: config::build_pipeline_id(provider_id, repo.id),
        provider_id,
        provider_type: config::PROVIDER_TYPE.to_string(),
        name: repo.name.clone(),
        status: latest.map_or(PipelineStatus::Pending, |run| map_status(&run.status)),
        last_run: latest.and_then(|run| timestamp(run.started).or(timestamp(run.created))),
        last_updated: Utc::now(),
        repository: repo.full_name.clone(),
        branch: latest.and_then(|run| non_empty(&run.branch)),
        workflow_file: non_empty(&repo.config_file),
        resource_state: None,
        metadata: HashMap::new(),
    }
}

pub(crate) fn map_run(run: &types::Pipeline, pipeline_id: &str, logs_url: String) -> PipelineRun {
    let started = timestamp(run.started);
    let finished = timestamp(run.finished);
    PipelineRun {
        id: format!("woodpecker-run-{}", run.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: run.number,
        status: map_status(&run.status),
        started_at: started.or(timestamp(run.created)).unwrap_or_else(Utc::now),
        concluded_at: finished,
        duration_seconds: started
            .zip(finished)
            .map(|(started, finished)| (finished - started).num_seconds()),
        logs_url,
        commit_sha: non_empty(&run.commit),
        commit_message: run
            .message
            .lines()
            .next()
            .map(str::trim)
            .and_then(non_empty),
        branch: non_empty(&run.branch),
        actor: non_empty(&run.author),
        inputs: non_empty(&run.branch).map(|branch| serde_json::json!({ BRANCH_INPUT: branch })),
        metadata: HashMap::from([("event".to_string(), serde_json::json!(run.event))]),
    }
}

fn step_log_availability(state: &str, started: i64) -> Option<LogAvailability> {
    match state {
        "running" => Some(LogAvailability::Partial),
        "pending" | "blocked" | "created" => Some(LogAvailability::Pending),
        "skipped" | "canceled" | "declined" => None,
        _ => timestamp(started).map(|_| LogAvailability::Ready),
    }
}

pub(crate) fn workflows_to_log_nodes(run: &types::Pipeline, run_url: &str) -> Vec<LogNode> {
    let mut nodes = Vec::new();
    for workflow in run.workflows.as_deref().unwrap_or_default() {
        let workflow_id = format!("workflow:{}", workflow.id);
        let steps = workflow.children.as_deref().unwrap_or_default();
        nodes.reserve(steps.len() + 1);
        nodes.push(LogNode {
            id: workflow_id.clone(),
            parent_id: None,
            kind: LogNodeKind::Stage,
            name: workflow.name.clone(),
            status: map_status(&workflow.state),
            started_at: timestamp(workflow.started),
            finished_at: timestamp(workflow.finished),
            log: None,
            web_url: None,
        });
        nodes.extend(steps.iter().map(|step| LogNode {
            id: step.id.to_string(),
            parent_id: Some(workflow_id.clone()),
            kind: LogNodeKind::Job,
            name: step.name.clone(),
            status: map_status(&step.state),
            started_at: timestamp(step.started),
            finished_at: timestamp(step.finished),
            log: step_log_availability(&step.state, step.started),
            web_url: Some(format!("{run_url}/{}", step.pid)),
        }));
    }
    nodes
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pipeline(value: serde_json::Value) -> types::Pipeline {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn every_woodpecker_status_maps_to_a_pipedash_status() {
        for (status, expected) in [
            ("success", PipelineStatus::Success),
            ("failure", PipelineStatus::Failed),
            ("error", PipelineStatus::Failed),
            ("killed", PipelineStatus::Cancelled),
            ("canceled", PipelineStatus::Cancelled),
            ("declined", PipelineStatus::Cancelled),
            ("skipped", PipelineStatus::Skipped),
            ("running", PipelineStatus::Running),
            ("pending", PipelineStatus::Pending),
            ("blocked", PipelineStatus::Pending),
            ("created", PipelineStatus::Pending),
            ("unknown", PipelineStatus::Pending),
        ] {
            assert_eq!(map_status(status), expected, "{status}");
        }
    }

    #[test]
    fn unset_timestamps_fall_back_to_created_and_leave_duration_empty() {
        let run = pipeline(json!({
            "id": 9,
            "number": 4,
            "status": "pending",
            "created": 1_761_645_600,
            "started": 0,
            "finished": 0,
            "message": "  fix build  \nbody",
        }));
        let mapped = map_run(&run, "woodpecker__1__5", String::new());
        assert_eq!(mapped.started_at.timestamp(), 1_761_645_600);
        assert_eq!(mapped.concluded_at, None);
        assert_eq!(mapped.duration_seconds, None);
        assert_eq!(mapped.commit_message.as_deref(), Some("fix build"));
        assert_eq!(mapped.branch, None);
        assert_eq!(mapped.inputs, None);
    }

    #[test]
    fn steps_nest_under_their_workflow_with_log_availability() {
        let run = pipeline(json!({
            "id": 9,
            "number": 4,
            "status": "running",
            "workflows": [{
                "id": 30,
                "name": "build",
                "state": "running",
                "children": [
                    {"id": 1, "pid": 2, "name": "clone", "state": "success", "started": 10},
                    {"id": 2, "pid": 3, "name": "test", "state": "running", "started": 11},
                    {"id": 3, "pid": 4, "name": "lint", "state": "pending"},
                    {"id": 4, "pid": 5, "name": "deploy", "state": "skipped"},
                    {"id": 5, "pid": 6, "name": "notify", "state": "success", "started": 0}
                ]
            }]
        }));
        let nodes = workflows_to_log_nodes(&run, "http://ci/repos/5/pipeline/4");
        let summary: Vec<_> = nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.parent_id.as_deref(),
                    node.kind,
                    node.log,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("workflow:30", None, LogNodeKind::Stage, None),
                (
                    "1",
                    Some("workflow:30"),
                    LogNodeKind::Job,
                    Some(LogAvailability::Ready)
                ),
                (
                    "2",
                    Some("workflow:30"),
                    LogNodeKind::Job,
                    Some(LogAvailability::Partial)
                ),
                (
                    "3",
                    Some("workflow:30"),
                    LogNodeKind::Job,
                    Some(LogAvailability::Pending)
                ),
                ("4", Some("workflow:30"), LogNodeKind::Job, None),
                ("5", Some("workflow:30"), LogNodeKind::Job, None),
            ]
        );
        assert_eq!(
            nodes[1].web_url.as_deref(),
            Some("http://ci/repos/5/pipeline/4/2")
        );
    }
}
