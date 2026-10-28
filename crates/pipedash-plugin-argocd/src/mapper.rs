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

pub(crate) fn operation_status(phase: &str) -> PipelineStatus {
    match phase {
        "Running" | "Terminating" => PipelineStatus::Running,
        "Succeeded" => PipelineStatus::Success,
        "Failed" | "Error" => PipelineStatus::Failed,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn app_url(server_url: &str, app_name: &str) -> String {
    format!(
        "{}/applications/{}",
        server_url.trim_end_matches('/'),
        app_name
    )
}

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

    let source_type = if source.chart.is_some() {
        "Helm"
    } else if let Some(path) = &source.path {
        if path.to_lowercase().contains("kustomize") || path.ends_with("kustomization.yaml") {
            "Kustomize"
        } else {
            "Plain"
        }
    } else {
        "Plain"
    };
    metadata.insert("source_type".to_string(), serde_json::json!(source_type));

    let repository = config::parse_repository_name(&source.repo_url);

    Pipeline {
        id: config::build_pipeline_id(provider_id, &namespace, &app.metadata.name),
        provider_id,
        provider_type: "argocd".to_string(),
        name: app.metadata.name.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository,
        branch: source.target_revision.clone(),
        workflow_file: source.path.clone(),
        metadata,
    }
}

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

    let logs_url = app_url(server_url, &app.metadata.name);

    let mut metadata = HashMap::new();
    metadata.insert(
        "sync_revision".to_string(),
        serde_json::json!(history.primary_revision()),
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

    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );
    metadata.insert(
        "destination_cluster".to_string(),
        serde_json::json!(app.spec.destination.cluster()),
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
        commit_sha: history.primary_revision().map(str::to_string),
        commit_message: None,
        branch: app.spec.primary_source().target_revision.clone(),
        actor: None,
        inputs: None,
        metadata,
    }
}

pub(crate) fn map_runs(
    app: &types::Application, provider_id: i64, server_url: &str, limit: usize,
) -> Vec<PipelineRun> {
    let operation = map_operation_to_run(app, provider_id, server_url);
    let operation_started = app.status.operation_state.as_ref().map(|op| op.started_at);
    let history = app
        .status
        .history
        .iter()
        .flatten()
        .rev()
        .filter(|entry| operation_started.is_none() || entry.deploy_started_at != operation_started)
        .take(limit)
        .map(|entry| map_history_to_run(entry, app, provider_id, server_url));

    operation.into_iter().chain(history).collect()
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

    let status = operation_status(&operation_state.phase);

    let duration = operation_state
        .finished_at
        .map(|finished| (finished - operation_state.started_at).num_seconds());

    let logs_url = app_url(server_url, &app.metadata.name);

    let mut metadata = HashMap::new();
    metadata.insert(
        "operation_phase".to_string(),
        serde_json::json!(operation_state.phase),
    );
    if let Some(ref msg) = operation_state.message {
        metadata.insert("operation_message".to_string(), serde_json::json!(msg));
    }

    let source = app.spec.primary_source();
    let source_type = if source.chart.is_some() {
        "Helm"
    } else if let Some(path) = &source.path {
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

    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );

    if let Some(path) = &source.path {
        metadata.insert("source_path".to_string(), serde_json::json!(path));
    }

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
        branch: source.target_revision.clone(),
        actor: None,
        inputs: None,
        metadata,
    })
}

pub(crate) fn map_available_pipeline(app: &types::Application) -> AvailablePipeline {
    let app_name = app.metadata.name.clone();
    let project_name = app.spec.project.clone();
    let source = app.spec.primary_source();

    let repo_full_name = config::parse_repository_name(&source.repo_url);

    let git_org_raw = config::extract_git_org(&source.repo_url);

    let (git_org, repo_name, unique_id) = if git_org_raw == "unknown" || git_org_raw.is_empty() {
        (None, None, app_name.clone())
    } else {
        (Some(git_org_raw), Some(repo_full_name), app_name.clone())
    };

    let cluster = app.spec.destination.cluster();
    let cluster_name = cluster
        .split("//")
        .last()
        .unwrap_or(cluster)
        .split(':')
        .next()
        .unwrap_or("unknown");

    let description = format!(
        "Project: {} | Path: {} | Target: {} | Namespace: {} | Cluster: {}",
        project_name,
        source.path.as_deref().unwrap_or("/"),
        source.target_revision.as_deref().unwrap_or("HEAD"),
        app.spec.destination.namespace.as_deref().unwrap_or("-"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app(spec: &str, status_extra: &str) -> types::Application {
        serde_json::from_str(&format!(
            r#"{{"metadata":{{"name":"web","namespace":"argocd"}},"spec":{spec},"status":{{"sync":{{"status":"Synced"}},"health":{{"status":"Healthy"}}{status_extra}}}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn multi_source_app_uses_first_source() {
        let app = app(
            r#"{"sources":[{"repoURL":"https://github.com/acme/web","path":"deploy","targetRevision":"release"},{"repoURL":"https://charts.acme.dev","chart":"redis","targetRevision":"18.1.0"}],"destination":{"server":"https://10.0.0.1:6443","namespace":"web"},"project":"default"}"#,
            r#","history":[{"deployedAt":"2026-10-20T10:00:00Z","id":3,"source":{"repoURL":""},"revisions":["abc123","18.1.0"]}]"#,
        );

        let pipeline = map_application_to_pipeline(&app, 1, "https://argocd.acme.dev");
        assert_eq!(pipeline.repository, "acme/web");
        assert_eq!(pipeline.branch.as_deref(), Some("release"));
        assert_eq!(pipeline.workflow_file.as_deref(), Some("deploy"));

        let history = &app.status.history.as_ref().unwrap()[0];
        let run = map_history_to_run(history, &app, 1, "https://argocd.acme.dev");
        assert_eq!(run.commit_sha.as_deref(), Some("abc123"));

        let available = map_available_pipeline(&app);
        assert_eq!(available.organization.as_deref(), Some("acme"));
        assert_eq!(
            available.description.as_deref(),
            Some("Project: default | Path: deploy | Target: release | Namespace: web | Cluster: 10.0.0.1")
        );
    }

    #[test]
    fn current_operation_replaces_its_own_history_entry() {
        let app = app(
            r#"{"source":{"repoURL":"https://github.com/acme/web","path":"deploy"},"destination":{"namespace":"web"},"project":"default"}"#,
            r#","history":[
                {"deployedAt":"2026-10-20T10:00:30Z","deployStartedAt":"2026-10-20T10:00:00Z","id":1,"revision":"abc"},
                {"deployedAt":"2026-10-20T11:00:30Z","deployStartedAt":"2026-10-20T11:00:00Z","id":2,"revision":"def"}],
              "operationState":{"operation":{"sync":{"revision":"def"}},"phase":"Succeeded",
                "startedAt":"2026-10-20T11:00:00Z","finishedAt":"2026-10-20T11:00:30Z"}"#,
        );

        let ids: Vec<_> = map_runs(&app, 1, "https://argocd.acme.dev", 10)
            .into_iter()
            .map(|run| run.id)
            .collect();

        assert_eq!(ids, ["argocd-sync-web-current", "argocd-sync-web-1"]);
    }

    #[test]
    fn app_with_destination_name_and_no_target_revision() {
        let app = app(
            r#"{"source":{"repoURL":"https://github.com/acme/web","path":"deploy"},"destination":{"name":"in-cluster"},"project":"default"}"#,
            "",
        );

        let pipeline = map_application_to_pipeline(&app, 1, "https://argocd.acme.dev");
        assert_eq!(pipeline.branch, None);
        assert_eq!(
            pipeline.metadata.get("destination_cluster"),
            Some(&serde_json::json!("in-cluster"))
        );

        let available = map_available_pipeline(&app);
        assert_eq!(
            available.description.as_deref(),
            Some("Project: default | Path: deploy | Target: HEAD | Namespace: - | Cluster: in-cluster")
        );
    }
}
