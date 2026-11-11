use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::utils::repository_name;
use pipedash_plugin_api::{
    Correlation,
    Pipeline,
    PipelineStatus,
    ResourceState,
};

use crate::{
    config,
    types,
};

pub(crate) mod catalog;
pub(crate) mod correlation;
pub(crate) mod runs;

fn operation_outcome(operation: &types::OperationState) -> Option<(PipelineStatus, &'static str)> {
    match operation.phase.as_str() {
        "Running" => Some((PipelineStatus::Running, "Syncing")),
        "Terminating" => Some((PipelineStatus::Running, "Terminating")),
        "Failed" | "Error" => Some((PipelineStatus::Failed, "Sync failed")),
        _ => None,
    }
}

fn or_unknown(value: &str) -> &str {
    if value.is_empty() {
        "Unknown"
    } else {
        value
    }
}

pub(crate) fn map_state(app: &types::Application) -> (PipelineStatus, ResourceState) {
    if let Some(operation) = &app.status.operation_state {
        if let Some((status, label)) = operation_outcome(operation) {
            return (
                status,
                ResourceState::new(label, operation.message.as_deref()),
            );
        }
    }

    let (status, label) = match (
        app.status.health.status.as_str(),
        app.status.sync.status.as_str(),
    ) {
        ("Degraded", _) => (PipelineStatus::Degraded, "Degraded"),
        ("Missing", _) => (PipelineStatus::Failed, "Missing"),
        ("Suspended", _) => (PipelineStatus::Cancelled, "Suspended"),
        ("Progressing", _) => (PipelineStatus::Running, "Progressing"),
        ("Healthy", "Synced") => (PipelineStatus::Success, "Synced"),
        ("Healthy", "OutOfSync") => (PipelineStatus::Pending, "Out of sync"),
        ("Healthy" | "", sync) => (PipelineStatus::Pending, or_unknown(sync)),
        (health, _) => (PipelineStatus::Pending, health),
    };
    (
        status,
        ResourceState::new(label, app.status.health.message.as_deref()),
    )
}

pub(crate) fn needs_resource_tree(app: &types::Application) -> bool {
    app.status
        .operation_state
        .as_ref()
        .is_none_or(|operation| operation_outcome(operation).is_none())
        && matches!(
            app.status.health.status.as_str(),
            "Progressing" | "Degraded" | "Unknown"
        )
}

pub(crate) fn degrade_from_tree(pipeline: &mut Pipeline, tree: &types::logs::ApplicationTree) {
    let degraded: Vec<_> = tree
        .nodes
        .iter()
        .filter(|node| {
            node.health
                .as_ref()
                .is_some_and(|health| health.status == "Degraded")
        })
        .collect();
    let Some(first) = degraded.iter().min_by_key(|node| node.kind != "Pod") else {
        return;
    };

    let mut message = if first.namespace.is_empty() {
        format!("{} {}", first.kind, first.name)
    } else {
        format!("{} {}/{}", first.kind, first.namespace, first.name)
    };
    if let Some(detail) = node_detail(first) {
        message = format!("{message}: {detail}");
    }
    if degraded.len() > 1 {
        message = format!("{message} (+{} more)", degraded.len() - 1);
    }

    pipeline.status = PipelineStatus::Degraded;
    pipeline.resource_state = Some(ResourceState::new("Degraded", Some(&message)));
}

fn node_detail(node: &types::logs::ResourceNode) -> Option<String> {
    if let Some(message) = node
        .health
        .as_ref()
        .and_then(|health| health.message.as_deref())
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return Some(message.to_string());
    }

    let info = |name: &str| {
        node.info
            .iter()
            .find(|item| item.name == name)
            .map(|item| item.value.trim())
            .filter(|value| !value.is_empty())
    };
    let parts: Vec<String> = [
        info("Status Reason").map(str::to_string),
        info("Containers").map(|ready| format!("{ready} containers ready")),
        info("Restart Count")
            .filter(|count| *count != "0")
            .map(|count| format!("{count} restarts")),
    ]
    .into_iter()
    .flatten()
    .collect();

    (!parts.is_empty()).then(|| parts.join(", "))
}

pub(crate) fn operation_status(phase: &str) -> PipelineStatus {
    match phase {
        "Running" | "Terminating" => PipelineStatus::Running,
        "Succeeded" => PipelineStatus::Success,
        "Failed" | "Error" => PipelineStatus::Failed,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn app_url(server_url: &str, app_name: &str) -> String {
    if server_url.is_empty() {
        return String::new();
    }
    format!(
        "{}/applications/{}",
        server_url.trim_end_matches('/'),
        app_name
    )
}

pub(crate) fn pipeline_id_of(app: &types::Application) -> String {
    let namespace = app.metadata.namespace.as_deref().unwrap_or("argocd");
    config::build_pipeline_id(namespace, &app.metadata.name)
}

pub(crate) fn map_application_to_pipeline(app: &types::Application, _server_url: &str) -> Pipeline {
    let (status, resource_state) = map_state(app);

    let last_run = app
        .status
        .operation_state
        .as_ref()
        .and_then(|op| op.finished_at)
        .or(app.metadata.creation_timestamp);

    let source = app.spec.primary_source();

    let mut metadata = HashMap::new();
    metadata.insert(
        "sync_status".to_string(),
        serde_json::json!(app.status.sync.status),
    );
    metadata.insert(
        "health_status".to_string(),
        serde_json::json!(app.status.health.status),
    );
    metadata.insert(
        "destination_cluster".to_string(),
        serde_json::json!(app.spec.destination.cluster()),
    );
    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );
    metadata.insert("repo_url".to_string(), serde_json::json!(source.repo_url));
    metadata.insert(
        "target_revision".to_string(),
        serde_json::json!(source.target_revision),
    );
    metadata.insert(
        "auto_sync_enabled".to_string(),
        serde_json::json!(app
            .spec
            .sync_policy
            .as_ref()
            .and_then(|sp| sp.automated.as_ref())
            .is_some()),
    );
    metadata.insert("project".to_string(), serde_json::json!(app.spec.project));

    if let Some(ref sync_policy) = app.spec.sync_policy {
        if let Some(ref automated) = sync_policy.automated {
            metadata.insert(
                "prune_enabled".to_string(),
                serde_json::json!(automated.prune.unwrap_or(false)),
            );
            metadata.insert(
                "self_heal_enabled".to_string(),
                serde_json::json!(automated.self_heal.unwrap_or(false)),
            );
        } else {
            metadata.insert("prune_enabled".to_string(), serde_json::json!(false));
            metadata.insert("self_heal_enabled".to_string(), serde_json::json!(false));
        }
    } else {
        metadata.insert("prune_enabled".to_string(), serde_json::json!(false));
        metadata.insert("self_heal_enabled".to_string(), serde_json::json!(false));
    }

    metadata.insert(
        "source_path".to_string(),
        serde_json::json!(source.path.as_deref().unwrap_or("/")),
    );

    if let Some(ref revision) = app.status.sync.revision {
        metadata.insert("current_revision".to_string(), serde_json::json!(revision));
    }

    if let Some(ref health_msg) = app.status.health.message {
        metadata.insert("health_message".to_string(), serde_json::json!(health_msg));
    }

    let out_of_sync_count = app
        .status
        .resources
        .iter()
        .filter(|r| {
            r.status
                .as_ref()
                .is_some_and(|s| !s.eq_ignore_ascii_case("Synced"))
        })
        .count();
    metadata.insert(
        "out_of_sync_count".to_string(),
        serde_json::json!(out_of_sync_count),
    );

    let total_resources = app.status.resources.len();
    let healthy_resources = app
        .status
        .resources
        .iter()
        .filter(|r| {
            r.health
                .as_ref()
                .is_some_and(|h| h.status.eq_ignore_ascii_case("Healthy"))
        })
        .count();
    metadata.insert(
        "resource_health_summary".to_string(),
        serde_json::json!(format!("{}/{} healthy", healthy_resources, total_resources)),
    );

    if let Some(history) = app.status.history.as_ref() {
        if let Some(last_sync) = history.first() {
            metadata.insert(
                "last_sync_time".to_string(),
                serde_json::json!(last_sync.deployed_at.to_rfc3339()),
            );
        }
    }

    metadata.insert("source_type".to_string(), serde_json::json!(source.kind()));

    let repository = repository_name(&source.repo_url);

    Pipeline {
        correlation: Correlation::default(),
        id: pipeline_id_of(app),
        name: app.metadata.name.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository,
        branch: source.target_revision.clone(),
        workflow_file: source.path.clone(),
        resource_state: Some(resource_state),
        metadata,
    }
}

#[cfg(test)]
mod tests;
