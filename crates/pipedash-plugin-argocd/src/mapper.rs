use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};

use crate::{
    config,
    types,
};

/// Map ArgoCD sync and health status to PipelineStatus
pub(crate) fn map_status(
    sync_status: &str, health_status: &str, operation_phase: Option<&str>,
) -> PipelineStatus {
    if let Some(phase) = operation_phase {
        match phase {
            "Running" | "Terminating" => return PipelineStatus::Running,
            "Failed" | "Error" => return PipelineStatus::Failed,
            _ => {} // Fall through to health status check
        }
    }

    match health_status {
        "Degraded" | "Missing" => PipelineStatus::Failed,
        "Suspended" => PipelineStatus::Cancelled,
        "Progressing" => PipelineStatus::Running,
        "Healthy" => match sync_status {
            "Synced" => PipelineStatus::Success,
            "OutOfSync" => PipelineStatus::Pending,
            _ => PipelineStatus::Pending,
        },
        _ => PipelineStatus::Pending,
    }
}

/// Map ArgoCD Application to Pipedash Pipeline
pub(crate) fn map_application_to_pipeline(
    app: &types::Application, provider_id: i64, _server_url: &str,
) -> Pipeline {
    let namespace = app
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "argocd".to_string());

    let operation_phase = app
        .status
        .operation_state
        .as_ref()
        .map(|op| op.phase.as_str());

    let status = map_status(
        &app.status.sync.status,
        &app.status.health.status,
        operation_phase,
    );

    let last_run = app
        .status
        .operation_state
        .as_ref()
        .and_then(|op| op.finished_at)
        .or(app.metadata.creation_timestamp);

    // Build metadata with ArgoCD-specific fields
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
        serde_json::json!(app.spec.destination.server),
    );
    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );
    metadata.insert(
        "repo_url".to_string(),
        serde_json::json!(app.spec.source.repo_url),
    );
    metadata.insert(
        "target_revision".to_string(),
        serde_json::json!(app.spec.source.target_revision),
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
        serde_json::json!(app.spec.source.path.as_deref().unwrap_or("/")),
    );

    if let Some(ref revision) = app.status.sync.revision {
        metadata.insert("current_revision".to_string(), serde_json::json!(revision));
    }

    if let Some(ref health_msg) = app.status.health.message {
        metadata.insert("health_message".to_string(), serde_json::json!(health_msg));
    }

    // Count out of sync resources
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

    // Calculate resource health summary (e.g., "5/7 healthy")
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

    // Last sync time from most recent history entry
    if let Some(history) = app.status.history.as_ref() {
        if let Some(last_sync) = history.first() {
            metadata.insert(
                "last_sync_time".to_string(),
                serde_json::json!(last_sync.deployed_at.to_rfc3339()),
            );
        }
    }

    // Detect source type (Helm, Kustomize, or Plain manifests)
    let source_type = if app.spec.source.chart.is_some() {
        "Helm"
    } else if let Some(ref path) = app.spec.source.path {
        if path.to_lowercase().contains("kustomize") || path.ends_with("kustomization.yaml") {
            "Kustomize"
        } else {
            "Plain"
        }
    } else {
        "Plain"
    };
    metadata.insert("source_type".to_string(), serde_json::json!(source_type));

    // Parse repository URL to get org/repo format
    let repository = config::parse_repository_name(&app.spec.source.repo_url);

    Pipeline {
        id: config::build_pipeline_id(provider_id, &namespace, &app.metadata.name),
        provider_id,
        provider_type: "argocd".to_string(),
        name: app.metadata.name.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository,
        branch: Some(app.spec.source.target_revision.clone()),
        workflow_file: app.spec.source.path.clone(),
        metadata,
    }
}

/// Map ArgoCD RevisionHistory to PipelineRun
pub(crate) fn map_history_to_run(
    history: &types::RevisionHistory, app: &types::Application, provider_id: i64, server_url: &str,
) -> PipelineRun {
    let namespace = app
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "argocd".to_string());

    let pipeline_id = config::build_pipeline_id(provider_id, &namespace, &app.metadata.name);

    let status = PipelineStatus::Success;

    let logs_url = format!(
        "{}/applications/{}",
        server_url.trim_end_matches('/'),
        app.metadata.name
    );

    let mut metadata = HashMap::new();
    metadata.insert(
        "sync_revision".to_string(),
        serde_json::json!(history.revision),
    );
    metadata.insert("history_id".to_string(), serde_json::json!(history.id));

    if let Some(ref source) = history.source {
        metadata.insert("source_path".to_string(), serde_json::json!(source.path));

        let sync_source_type = if source.chart.is_some() {
            "Helm"
        } else if let Some(ref path) = source.path {
            if path.to_lowercase().contains("kustomize") || path.ends_with("kustomization.yaml") {
                "Kustomize"
            } else {
                "Plain"
            }
        } else {
            "Plain"
        };
        metadata.insert(
            "source_type".to_string(),
            serde_json::json!(sync_source_type),
        );

        if let Some(ref chart) = source.chart {
            metadata.insert("helm_chart".to_string(), serde_json::json!(chart));
        }
    }

    metadata.insert(
        "app_sync_status".to_string(),
        serde_json::json!(app.status.sync.status),
    );
    metadata.insert(
        "app_health_status".to_string(),
        serde_json::json!(app.status.health.status),
    );

    // Destination information
    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );
    metadata.insert(
        "destination_cluster".to_string(),
        serde_json::json!(app.spec.destination.server),
    );

    let run_number = history.deployed_at.timestamp();

    PipelineRun {
        id: format!("argocd-sync-{}-{}", app.metadata.name, history.id),
        pipeline_id,
        run_number,
        status,
        started_at: history.deployed_at,
        concluded_at: Some(history.deployed_at),
        duration_seconds: None,
        logs_url,
        commit_sha: Some(history.revision.clone()),
        commit_message: None,
        branch: Some(app.spec.source.target_revision.clone()),
        actor: None,
        inputs: None,
        metadata,
    }
}

pub(crate) fn map_operation_to_run(
    app: &types::Application, provider_id: i64, server_url: &str,
) -> Option<PipelineRun> {
    let operation_state = app.status.operation_state.as_ref()?;

    let namespace = app
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "argocd".to_string());

    let pipeline_id = config::build_pipeline_id(provider_id, &namespace, &app.metadata.name);

    let status = match operation_state.phase.as_str() {
        "Running" | "Terminating" => PipelineStatus::Running,
        "Succeeded" => PipelineStatus::Success,
        "Failed" | "Error" => PipelineStatus::Failed,
        _ => PipelineStatus::Pending,
    };

    let duration = operation_state
        .finished_at
        .map(|finished| (finished - operation_state.started_at).num_seconds());

    let logs_url = format!(
        "{}/applications/{}",
        server_url.trim_end_matches('/'),
        app.metadata.name
    );

    let mut metadata = HashMap::new();
    metadata.insert(
        "operation_phase".to_string(),
        serde_json::json!(operation_state.phase),
    );
    if let Some(ref msg) = operation_state.message {
        metadata.insert("operation_message".to_string(), serde_json::json!(msg));
    }

    let source_type = if app.spec.source.chart.is_some() {
        "Helm"
    } else if let Some(ref path) = app.spec.source.path {
        if path.to_lowercase().contains("kustomize") || path.ends_with("kustomization.yaml") {
            "Kustomize"
        } else {
            "Plain"
        }
    } else {
        "Plain"
    };
    metadata.insert("source_type".to_string(), serde_json::json!(source_type));

    metadata.insert(
        "app_sync_status".to_string(),
        serde_json::json!(app.status.sync.status),
    );
    metadata.insert(
        "app_health_status".to_string(),
        serde_json::json!(app.status.health.status),
    );

    // Destination
    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );

    // Source path
    if let Some(ref path) = app.spec.source.path {
        metadata.insert("source_path".to_string(), serde_json::json!(path));
    }

    // Use timestamp as run_number for consistent sorting with history
    // Current operations use started_at timestamp to sort chronologically
    let run_number = operation_state.started_at.timestamp();

    Some(PipelineRun {
        id: format!("argocd-sync-{}-current", app.metadata.name),
        pipeline_id,
        run_number,
        status,
        started_at: operation_state.started_at,
        concluded_at: operation_state.finished_at,
        duration_seconds: duration,
        logs_url,
        commit_sha: app.status.sync.revision.clone(),
        commit_message: None,
        branch: Some(app.spec.source.target_revision.clone()),
        actor: None,
        inputs: None,
        metadata,
    })
}

/// Map Application to AvailablePipeline (for pipeline discovery)
pub(crate) fn map_available_pipeline(app: &types::Application) -> AvailablePipeline {
    let app_name = app.metadata.name.clone();
    let project_name = app.spec.project.clone();

    let repo_full_name = config::parse_repository_name(&app.spec.source.repo_url);

    let git_org_raw = config::extract_git_org(&app.spec.source.repo_url);

    let (git_org, repo_name, unique_id) = if git_org_raw == "unknown" || git_org_raw.is_empty() {
        (None, None, app_name.clone())
    } else {
        (Some(git_org_raw), Some(repo_full_name), app_name.clone())
    };

    let cluster_name = app
        .spec
        .destination
        .server
        .split("//")
        .last()
        .unwrap_or(&app.spec.destination.server)
        .split(':')
        .next()
        .unwrap_or("unknown");

    let description = format!(
        "Project: {} | Path: {} | Target: {} | Namespace: {} | Cluster: {}",
        project_name,
        app.spec.source.path.as_deref().unwrap_or("/"),
        app.spec.source.target_revision,
        app.spec.destination.namespace,
        cluster_name
    );

    AvailablePipeline {
        id: unique_id,
        name: app_name,
        description: Some(description),
        organization: git_org,
        repository: repo_name,
    }
}
