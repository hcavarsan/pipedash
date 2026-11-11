use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::utils::deployed_status;
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Observed,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    ResourceState,
    RunEvidence,
};
use serde_json::{
    json,
    Value,
};

use crate::types::{
    StackInfo,
    StackListItem,
    Update,
    UpdateListItem,
    UpdateLogs,
};

mod correlation;

pub(crate) fn map_stack_state(info: &StackInfo) -> (PipelineStatus, ResourceState) {
    let (status, label) = match info.state.as_str() {
        "running" => (PipelineStatus::Success, "Up"),
        "deploying" => (PipelineStatus::Running, "Deploying"),
        "removing" => (PipelineStatus::Running, "Removing"),
        "restarting" => (PipelineStatus::Degraded, "Restarting"),
        "unhealthy" => (PipelineStatus::Degraded, "Unhealthy"),
        "dead" => (PipelineStatus::Failed, "Dead"),
        "paused" => (PipelineStatus::Cancelled, "Paused"),
        "stopped" => (PipelineStatus::Cancelled, "Stopped"),
        "down" => (PipelineStatus::Cancelled, "Down"),
        "created" => (PipelineStatus::Pending, "Created"),
        _ => (PipelineStatus::Pending, "Unknown"),
    };
    let message = state_explanation(info);
    (status, ResourceState::new(label, Some(&message)))
}

fn state_explanation(info: &StackInfo) -> String {
    if info.state == "running" {
        return String::new();
    }
    let mut parts = Vec::new();
    if info.project_missing {
        parts.push("Project is missing on the host".to_string());
    }
    if !info.missing_files.is_empty() {
        parts.push(format!("Missing files: {}", info.missing_files.join(", ")));
    }
    parts.extend(
        info.status
            .iter()
            .filter(|status| !status.is_empty())
            .cloned(),
    );
    parts.join("; ")
}

pub(crate) fn map_update_status(status: &str, success: bool) -> PipelineStatus {
    match (status, success) {
        ("Queued", _) => PipelineStatus::Pending,
        ("InProgress", _) => PipelineStatus::Running,
        ("Complete", true) => PipelineStatus::Success,
        ("Complete", false) => PipelineStatus::Failed,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn run_url(server_url: &str, stack_id: &str, update_id: &str) -> String {
    if update_id.is_empty() {
        format!("{server_url}/stacks/{stack_id}")
    } else {
        format!("{server_url}/updates/{update_id}")
    }
}

fn timestamp(millis: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(millis).unwrap_or_default()
}

fn optional_timestamp(millis: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(millis).filter(|_| millis > 0)
}

fn repository(stack: &StackListItem) -> String {
    match stack.info.host() {
        "" => stack.name.clone(),
        host => format!("{host}/{}", stack.name),
    }
}

pub(crate) fn map_pipeline(stack: &StackListItem, last_update_start: Option<i64>) -> Pipeline {
    let info = &stack.info;
    let (status, resource_state) = map_stack_state(info);
    let mut metadata = HashMap::from([
        ("stack_id".to_string(), json!(stack.id)),
        (
            "update_available".to_string(),
            json!(info.update_available()),
        ),
        ("service_count".to_string(), json!(info.services.len())),
        ("project_missing".to_string(), json!(info.project_missing)),
        ("missing_files".to_string(), json!(info.missing_files.len())),
    ]);
    let optional = [
        ("server", Some(info.host())),
        ("repo", Some(info.repo.as_str())),
        ("repo_url", Some(info.repo_link.as_str())),
        ("docker_status", info.status.as_deref()),
        ("deployed_hash", info.deployed_hash.as_deref()),
        ("latest_hash", info.latest_hash.as_deref()),
    ];
    for (key, value) in optional {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            metadata.insert(key.to_string(), json!(value));
        }
    }

    let branch = (!info.repo.is_empty() && !info.branch.is_empty()).then(|| info.branch.clone());

    Pipeline {
        correlation: correlation::pipeline(stack),
        id: stack.id.clone(),
        name: stack.name.clone(),
        status,
        last_run: last_update_start.map(timestamp),
        last_updated: Utc::now(),
        repository: repository(stack),
        branch,
        workflow_file: None,
        resource_state: Some(resource_state),
        metadata,
    }
}

pub(crate) fn map_available_pipeline(stack: &StackListItem) -> AvailablePipeline {
    let info = &stack.info;
    let mut details = vec![format!("State: {}", info.state)];
    if !info.host().is_empty() {
        details.push(format!("Server: {}", info.host()));
    }
    if !info.repo.is_empty() {
        details.push(format!("Repo: {}@{}", info.repo, info.branch));
    }

    AvailablePipeline {
        id: stack.id.clone(),
        name: stack.name.clone(),
        description: Some(details.join(" | ")),
        organization: Some(info.host().to_string()).filter(|host| !host.is_empty()),
        repository: Some(info.repo.clone()).filter(|repo| !repo.is_empty()),
    }
}

pub(crate) fn map_run(
    stack_id: &str, server_url: &str, item: &UpdateListItem, detail: Option<&Update>,
    info: Option<&StackInfo>,
) -> PipelineRun {
    let (status, success) = detail.map_or((item.status.as_str(), item.success), |update| {
        (update.status.as_str(), update.success)
    });
    let started_at = timestamp(item.start_ts);
    let end_ts = detail.and_then(|update| update.end_ts);
    let actor = [&item.username, &item.operator]
        .into_iter()
        .find(|name| !name.is_empty())
        .cloned();

    let mut metadata: HashMap<String, Value> = HashMap::from([
        ("operation".to_string(), json!(item.operation)),
        ("update_status".to_string(), json!(status)),
        ("operator".to_string(), json!(item.operator)),
    ]);
    if !item.id.is_empty() {
        metadata.insert("update_id".to_string(), json!(item.id));
    }

    PipelineRun {
        correlation: correlation::run(info, detail),
        evidence: RunEvidence {
            timing: if end_ts.is_some() {
                Observed::Recorded
            } else {
                Observed::Absent
            },
            ..RunEvidence::default()
        },
        id: format!("{stack_id}__{}", item.id),
        pipeline_id: stack_id.to_string(),
        run_number: item.start_ts,
        status: map_update_status(status, success),
        started_at,
        concluded_at: end_ts.map(timestamp),
        duration_seconds: end_ts.map(|end| (end - item.start_ts).max(0) / 1000),
        logs_url: run_url(server_url, stack_id, &item.id),
        commit_sha: detail
            .map(|update| update.commit_hash.clone())
            .filter(|hash| !hash.is_empty()),
        commit_message: None,
        branch: None,
        actor,
        inputs: None,
        metadata,
    }
}

pub(crate) fn apply_live_status(run: &mut PipelineRun, live: PipelineStatus) {
    let status = deployed_status(run.status, live);
    if status != run.status {
        run.status = status;
        run.evidence.outcome = Observed::Absent;
        run.evidence.attempt = false;
    }
}

pub(crate) fn map_log_nodes(update: &UpdateLogs, web_url: &str) -> Vec<LogNode> {
    let last = update.logs.len().saturating_sub(1);
    update
        .logs
        .iter()
        .enumerate()
        .map(|(index, log)| {
            let open = update.status == "InProgress" && index == last;
            let started_at = optional_timestamp(log.start_ts);
            let name = match log.stage.trim() {
                "" => format!("Stage {}", index + 1),
                stage => stage.to_string(),
            };
            LogNode {
                id: index.to_string(),
                parent_id: None,
                kind: LogNodeKind::Job,
                name,
                status: map_update_status(
                    if open { "InProgress" } else { "Complete" },
                    log.success,
                ),
                started_at,
                finished_at: started_at
                    .and(optional_timestamp(log.end_ts))
                    .filter(|_| !open),
                log: Some(if open {
                    LogAvailability::Partial
                } else {
                    LogAvailability::Ready
                }),
                web_url: Some(web_url.to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
