use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    CommitSha,
    Correlation,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RepoId,
    RunEvidence,
};

use crate::types;

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

pub(crate) fn source_repo(repo: &types::Repo) -> Option<RepoId> {
    [repo.forge_url.as_deref(), repo.clone_url.as_deref()]
        .into_iter()
        .flatten()
        .find_map(RepoId::parse)
}

pub(crate) fn map_pipeline(repo: &types::Repo, latest: Option<&types::Pipeline>) -> Pipeline {
    Pipeline {
        correlation: Correlation::single_source(source_repo(repo), None),
        id: repo.id.to_string(),
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

pub(crate) fn map_run(
    run: &types::Pipeline, pipeline_id: &str, logs_url: String, repo: Option<&RepoId>,
) -> PipelineRun {
    let started = timestamp(run.started);
    let finished = timestamp(run.finished);
    PipelineRun {
        correlation: Correlation::single_source(repo.cloned(), CommitSha::parse(&run.commit)),
        evidence: RunEvidence::default(),
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
mod tests;
