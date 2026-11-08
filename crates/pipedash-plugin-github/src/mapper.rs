use std::collections::HashMap;

use chrono::Utc;
use octocrab::models::workflows::Job;
use pipedash_plugin_api::utils::{
    duration_seconds,
    status_from_conclusion,
};
use pipedash_plugin_api::{
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineRun,
    PipelineStatus,
    ResourceState,
};
use serde::Serialize;
use tracing::debug;

use crate::types;

pub(crate) fn map_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    status_from_conclusion(
        status,
        conclusion,
        &["success", "neutral"],
        &["cancelled", "stale"],
    )
}

pub(crate) fn disabled_state(workflow_state: &str) -> Option<ResourceState> {
    let reason = match workflow_state {
        "disabled_inactivity" => {
            "GitHub disabled this workflow after 60 days without repository activity"
        }
        "disabled_manually" => "Someone disabled this workflow on GitHub",
        "disabled_fork" => "GitHub disables workflows in a fork until you enable them",
        _ => return None,
    };
    Some(ResourceState::new("Disabled", Some(reason)))
}

fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) fn jobs_to_log_nodes(jobs: &[Job]) -> Vec<LogNode> {
    let mut nodes = Vec::with_capacity(jobs.iter().map(|job| job.steps.len() + 1).sum());

    for job in jobs {
        let job_id = job.id.to_string();
        let status_wire = wire(&job.status);
        let not_started = matches!(
            status_wire.as_str(),
            "queued" | "waiting" | "pending" | "requested"
        );

        nodes.push(LogNode {
            id: job_id.clone(),
            parent_id: None,
            kind: LogNodeKind::Job,
            name: job.name.clone(),
            status: map_status(&status_wire, job.conclusion.as_ref().map(wire).as_deref()),
            started_at: (!not_started).then_some(job.started_at),
            finished_at: job.completed_at,
            log: Some(if status_wire == "completed" {
                LogAvailability::Ready
            } else {
                LogAvailability::Pending
            }),
            web_url: Some(job.html_url.to_string()),
        });

        for step in &job.steps {
            nodes.push(LogNode::step(
                &job_id,
                step.number,
                step.name.clone(),
                map_status(
                    &wire(&step.status),
                    step.conclusion.as_ref().map(wire).as_deref(),
                ),
                step.started_at,
                step.completed_at,
            ));
        }
    }

    nodes
}

pub(crate) fn run_to_pipeline_run(run: types::Run, pipeline_id: &str) -> PipelineRun {
    let status = map_status(run.status.as_str(), run.conclusion.as_deref());
    let started_at = run.created_at.with_timezone(&Utc);
    let concluded_at = run.updated_at.with_timezone(&Utc);

    let mut inputs_map = serde_json::Map::new();

    inputs_map.insert(
        "ref".to_string(),
        serde_json::Value::String(run.head_branch.clone()),
    );

    debug!(
        "Run #{}: branch={}, event={:?}",
        run.run_number, run.head_branch, run.event
    );

    let inputs = Some(serde_json::Value::Object(inputs_map));

    let mut metadata = HashMap::new();
    metadata.insert("event".to_string(), serde_json::json!(&run.event));
    metadata.insert("run_id".to_string(), serde_json::json!(run.id.0));

    if run.status == "waiting" {
        metadata.insert("awaiting_review".to_string(), serde_json::json!(true));
    }

    if run.status == "completed"
        && matches!(run.conclusion.as_deref(), Some("failure" | "timed_out"))
    {
        metadata.insert("can_rerun_failed_jobs".to_string(), serde_json::json!(true));
    }

    if let Some(owner) = run.repository.owner.as_ref() {
        metadata.insert("owner".to_string(), serde_json::json!(&owner.login));
    }

    PipelineRun {
        id: format!("github-run-{}", run.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: run.run_number,
        status,
        started_at,
        concluded_at: Some(concluded_at),
        duration_seconds: duration_seconds(Some(started_at), Some(concluded_at)),
        logs_url: run.html_url.to_string(),
        commit_sha: Some(run.head_sha.clone()),
        commit_message: Some(run.head_commit.message.clone()),
        branch: Some(run.head_branch.clone()),
        actor: Some(run.head_commit.author.name.clone()),
        inputs,
        metadata,
    }
}

#[cfg(test)]
mod tests;
