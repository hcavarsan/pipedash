use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::utils::duration_seconds;
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    ResourceState,
};
use serde_json::{
    json,
    Value,
};

use crate::types::{
    Application,
    Deployment,
    DeploymentDetail,
};

pub(crate) const LOG_NODE_ID: &str = "deployment";

pub(crate) struct Placement {
    pub project: String,
    pub environment: String,
}

pub(crate) fn map_app_state(status: &str) -> (PipelineStatus, ResourceState) {
    let (state, health) = status.split_once(':').unwrap_or((status, ""));
    let (status, label) = match (state, health) {
        ("running", "unhealthy") => (PipelineStatus::Degraded, "Unhealthy"),
        ("running", _) => (PipelineStatus::Success, "Up"),
        ("degraded", _) => (PipelineStatus::Degraded, "Degraded"),
        ("restarting", _) => (PipelineStatus::Degraded, "Restarting"),
        ("dead", _) => (PipelineStatus::Failed, "Dead"),
        ("starting", _) => (PipelineStatus::Running, "Starting"),
        ("exited", _) => (PipelineStatus::Cancelled, "Exited"),
        ("paused", _) => (PipelineStatus::Cancelled, "Paused"),
        _ => (PipelineStatus::Pending, "Unknown"),
    };
    (status, ResourceState::new(label, None))
}

pub(crate) fn map_deployment_status(status: &str) -> PipelineStatus {
    match status {
        "queued" => PipelineStatus::Pending,
        "in_progress" => PipelineStatus::Running,
        "finished" => PipelineStatus::Success,
        "failed" => PipelineStatus::Failed,
        "cancelled-by-user" => PipelineStatus::Cancelled,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn concluded_at(
    status: &str, finished_at: Option<DateTime<Utc>>, updated_at: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match status {
        "finished" | "failed" | "cancelled-by-user" => finished_at.or(updated_at),
        _ => None,
    }
}

pub(crate) fn web_url(server_url: &str, deployment_url: Option<&str>) -> String {
    match deployment_url.filter(|path| !path.is_empty()) {
        Some(path) => format!("{server_url}{path}"),
        None => server_url.to_string(),
    }
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn is_docker_image(app: &Application) -> bool {
    app.build_pack.as_deref() == Some("dockerimage")
}

fn repository(app: &Application) -> String {
    if is_docker_image(app) {
        let image = app
            .docker_registry_image_name
            .as_deref()
            .unwrap_or_default();
        return match nonempty(app.docker_registry_image_tag.as_deref()) {
            Some(tag) => format!("{image}:{tag}"),
            None => image.to_string(),
        };
    }
    nonempty(app.git_repository.as_deref())
        .or_else(|| app.server_name())
        .unwrap_or_default()
        .to_string()
}

fn branch(app: &Application) -> Option<String> {
    if is_docker_image(app) || nonempty(app.git_repository.as_deref()).is_none() {
        return None;
    }
    nonempty(app.git_branch.as_deref()).map(str::to_string)
}

fn source(deployment: &Deployment) -> &'static str {
    if deployment.pull_request_id > 0 {
        "Pull request"
    } else if deployment.is_webhook {
        "Webhook"
    } else if deployment.rollback {
        "Rollback"
    } else if deployment.is_api {
        "API"
    } else {
        "Manual"
    }
}

pub(crate) fn map_pipeline(app: &Application, latest: Option<&Deployment>) -> Pipeline {
    let (live, state) = map_app_state(&app.status);
    let mut metadata = HashMap::from([
        ("application_uuid".to_string(), json!(app.uuid)),
        ("state".to_string(), json!(state.label)),
    ]);
    let (status, resource_state) = match latest.map(|deployment| deployment.status.as_str()) {
        None | Some("finished") => (live, Some(state)),
        Some(other) => (map_deployment_status(other), None),
    };

    let optional = [
        ("server", app.server_name()),
        ("build_pack", app.build_pack.as_deref()),
        ("app_status", Some(app.status.as_str())),
        ("domains", app.fqdn.as_deref()),
        (
            "deployment_status",
            latest.map(|deployment| deployment.status.as_str()),
        ),
    ];
    for (key, value) in optional {
        if let Some(value) = nonempty(value) {
            metadata.insert(key.to_string(), json!(value));
        }
    }

    Pipeline {
        id: app.uuid.clone(),
        name: app.name.clone(),
        status,
        last_run: latest.map(|deployment| deployment.created_at),
        last_updated: Utc::now(),
        repository: repository(app),
        branch: branch(app),
        workflow_file: None,
        resource_state,
        metadata,
    }
}

pub(crate) fn map_available_pipeline(
    app: &Application, placement: Option<&Placement>,
) -> AvailablePipeline {
    let repository = repository(app);
    let mut details = vec![format!("State: {}", app.status)];
    if let Some(placement) = placement {
        details.push(format!("Environment: {}", placement.environment));
    }
    if !repository.is_empty() {
        match branch(app) {
            Some(branch) => details.push(format!("Source: {repository}@{branch}")),
            None => details.push(format!("Source: {repository}")),
        }
    }

    AvailablePipeline {
        id: app.uuid.clone(),
        name: app.name.clone(),
        description: Some(details.join(" | ")),
        organization: placement.map(|placement| placement.project.clone()),
        repository: Some(repository).filter(|repository| !repository.is_empty()),
    }
}

pub(crate) fn map_run(pipeline_id: &str, server_url: &str, deployment: &Deployment) -> PipelineRun {
    let started_at = deployment.created_at;
    let concluded = concluded_at(
        &deployment.status,
        deployment.finished_at,
        deployment.updated_at,
    );

    let mut metadata: HashMap<String, Value> = HashMap::from([
        (
            "deployment_uuid".to_string(),
            json!(deployment.deployment_uuid),
        ),
        ("source".to_string(), json!(source(deployment))),
        ("deployment_status".to_string(), json!(deployment.status)),
    ]);
    if let Some(server) = nonempty(deployment.server_name.as_deref()) {
        metadata.insert("server_name".to_string(), json!(server));
    }
    if deployment.pull_request_id > 0 {
        metadata.insert(
            "pull_request_id".to_string(),
            json!(deployment.pull_request_id),
        );
    }

    PipelineRun {
        id: format!("{pipeline_id}__{}", deployment.deployment_uuid),
        pipeline_id: pipeline_id.to_string(),
        run_number: deployment.id,
        status: map_deployment_status(&deployment.status),
        started_at,
        concluded_at: concluded,
        duration_seconds: duration_seconds(Some(started_at), concluded)
            .map(|seconds| seconds.max(0)),
        logs_url: web_url(server_url, deployment.deployment_url.as_deref()),
        commit_sha: nonempty(deployment.commit.as_deref())
            .filter(|commit| *commit != "HEAD")
            .map(str::to_string),
        commit_message: nonempty(deployment.commit_message.as_deref()).map(str::to_string),
        branch: None,
        actor: None,
        inputs: None,
        metadata,
    }
}

pub(crate) fn map_log_node(detail: &DeploymentDetail, web_url: &str) -> LogNode {
    LogNode {
        id: LOG_NODE_ID.to_string(),
        parent_id: None,
        kind: LogNodeKind::Job,
        name: "Deployment".to_string(),
        status: map_deployment_status(&detail.status),
        started_at: Some(detail.created_at),
        finished_at: concluded_at(&detail.status, detail.finished_at, detail.updated_at),
        log: Some(match detail.status.as_str() {
            "queued" => LogAvailability::Pending,
            "in_progress" => LogAvailability::Partial,
            _ => LogAvailability::Ready,
        }),
        web_url: Some(web_url.to_string()),
    }
}

#[cfg(test)]
mod tests;
