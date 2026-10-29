use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    ResourceState,
};

use crate::{
    config,
    types,
};

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

pub(crate) fn degrade_from_tree(pipeline: &mut Pipeline, tree: &types::ApplicationTree) {
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

fn node_detail(node: &types::ResourceNode) -> Option<String> {
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
        resource_state: Some(resource_state),
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

    fn state_app(
        sync: &str, health: &str, health_message: Option<&str>,
        operation: Option<(&str, Option<&str>)>,
    ) -> types::Application {
        let operation = operation.map(|(phase, message)| {
            serde_json::json!({"phase": phase, "message": message, "startedAt": "2026-10-27T10:00:00Z"})
        });
        serde_json::from_value(serde_json::json!({
            "metadata": {"name": "web", "namespace": "argocd"},
            "spec": {"source": {"repoURL": "https://github.com/acme/web"}, "destination": {"namespace": "web"}, "project": "default"},
            "status": {
                "sync": {"status": sync},
                "health": {"status": health, "message": health_message},
                "operationState": operation,
            },
        }))
        .unwrap()
    }

    #[test]
    fn state_follows_operation_then_health_then_sync() {
        let cases = [
            (
                "Synced",
                "Healthy",
                Some("Running"),
                PipelineStatus::Running,
                "Syncing",
            ),
            (
                "Synced",
                "Healthy",
                Some("Terminating"),
                PipelineStatus::Running,
                "Terminating",
            ),
            (
                "Synced",
                "Healthy",
                Some("Failed"),
                PipelineStatus::Failed,
                "Sync failed",
            ),
            (
                "Synced",
                "Healthy",
                Some("Error"),
                PipelineStatus::Failed,
                "Sync failed",
            ),
            (
                "Synced",
                "Healthy",
                Some("Succeeded"),
                PipelineStatus::Success,
                "Synced",
            ),
            (
                "OutOfSync",
                "Degraded",
                None,
                PipelineStatus::Degraded,
                "Degraded",
            ),
            ("Synced", "Missing", None, PipelineStatus::Failed, "Missing"),
            (
                "Synced",
                "Suspended",
                None,
                PipelineStatus::Cancelled,
                "Suspended",
            ),
            (
                "Synced",
                "Progressing",
                None,
                PipelineStatus::Running,
                "Progressing",
            ),
            ("Synced", "Healthy", None, PipelineStatus::Success, "Synced"),
            (
                "OutOfSync",
                "Healthy",
                None,
                PipelineStatus::Pending,
                "Out of sync",
            ),
            (
                "Unknown",
                "Healthy",
                None,
                PipelineStatus::Pending,
                "Unknown",
            ),
            (
                "Synced",
                "Unknown",
                None,
                PipelineStatus::Pending,
                "Unknown",
            ),
            ("OutOfSync", "", None, PipelineStatus::Pending, "OutOfSync"),
            ("", "", None, PipelineStatus::Pending, "Unknown"),
            ("Synced", "Custom", None, PipelineStatus::Pending, "Custom"),
        ];

        for (sync, health, phase, status, label) in cases {
            let app = state_app(sync, health, None, phase.map(|phase| (phase, None)));
            let (actual, state) = map_state(&app);
            assert_eq!(
                (actual, state.label.as_str()),
                (status, label),
                "{sync}/{health}/{phase:?}"
            );
        }
    }

    #[test]
    fn state_message_comes_from_the_operation_or_the_health() {
        let syncing = state_app(
            "Synced",
            "Progressing",
            Some("waiting for rollout"),
            Some(("Running", Some("one or more tasks are running"))),
        );
        assert_eq!(
            map_state(&syncing).1,
            ResourceState::new("Syncing", Some("one or more tasks are running"))
        );

        let degraded = state_app("Synced", "Degraded", Some("  deployment web failed "), None);
        assert_eq!(
            map_state(&degraded).1,
            ResourceState::new("Degraded", Some("deployment web failed"))
        );

        let quiet = state_app("Synced", "Healthy", Some(""), None);
        assert_eq!(map_state(&quiet).1.message, None);

        let pipeline = map_application_to_pipeline(&syncing, 1, "https://argocd.acme.dev");
        assert_eq!(pipeline.status, PipelineStatus::Running);
        assert_eq!(pipeline.resource_state.unwrap().label, "Syncing");
    }

    #[test]
    fn resource_tree_is_only_needed_for_unhealthy_idle_apps() {
        let cases = [
            ("Synced", "Healthy", None, false),
            ("Synced", "Progressing", None, true),
            ("Synced", "Degraded", None, true),
            ("Synced", "Unknown", None, true),
            ("Synced", "Suspended", None, false),
            ("Synced", "Missing", None, false),
            ("Synced", "Progressing", Some("Running"), false),
            ("Synced", "Progressing", Some("Terminating"), false),
            ("Synced", "Degraded", Some("Failed"), false),
            ("Synced", "Progressing", Some("Succeeded"), true),
        ];

        for (sync, health, phase, needed) in cases {
            let app = state_app(sync, health, None, phase.map(|phase| (phase, None)));
            assert_eq!(needs_resource_tree(&app), needed, "{health}/{phase:?}");
        }
    }

    fn tree(nodes: &str) -> types::ApplicationTree {
        serde_json::from_str(&format!(r#"{{"nodes":{nodes}}}"#)).unwrap()
    }

    fn progressing_pipeline() -> Pipeline {
        let app = state_app("Synced", "Progressing", Some("waiting"), None);
        map_application_to_pipeline(&app, 1, "https://argocd.acme.dev")
    }

    #[test]
    fn degraded_pod_names_itself_and_counts_the_rest() {
        let mut pipeline = progressing_pipeline();
        degrade_from_tree(
            &mut pipeline,
            &tree(
                r#"[
                {"kind":"Deployment","namespace":"web","name":"web","health":{"status":"Degraded","message":"not progressing"}},
                {"kind":"Pod","namespace":"web","name":"web-ok","health":{"status":"Healthy"}},
                {"kind":"Pod","namespace":"web","name":"web-7f9c-abc","health":{"status":"Degraded","message":"back-off 40s restarting failed container=app"}},
                {"kind":"Pod","namespace":"web","name":"web-7f9c-def","health":{"status":"Degraded"}},
                {"kind":"Service","namespace":"web","name":"web"}]"#,
            ),
        );

        assert_eq!(pipeline.status, PipelineStatus::Degraded);
        assert_eq!(
            pipeline.resource_state,
            Some(ResourceState::new(
                "Degraded",
                Some(
                    "Pod web/web-7f9c-abc: back-off 40s restarting failed container=app (+2 more)"
                )
            ))
        );
    }

    #[test]
    fn degraded_non_pod_without_message_or_namespace() {
        let mut pipeline = progressing_pipeline();
        degrade_from_tree(
            &mut pipeline,
            &tree(r#"[{"kind":"ClusterRole","name":"web","health":{"status":"Degraded"}}]"#),
        );

        assert_eq!(
            pipeline.resource_state,
            Some(ResourceState::new("Degraded", Some("ClusterRole web")))
        );
    }

    #[test]
    fn degraded_pod_without_health_message_is_explained_by_its_info() {
        let mut pipeline = progressing_pipeline();
        degrade_from_tree(
            &mut pipeline,
            &tree(
                r#"[{"kind":"Pod","namespace":"web","name":"web-5884","health":{"status":"Degraded"},
                "info":[{"name":"Status Reason","value":"CrashLoopBackOff"},{"name":"Node","value":"kind"},
                {"name":"Containers","value":"0/1"},{"name":"Restart Count","value":"4"}]}]"#,
            ),
        );

        assert_eq!(
            pipeline.resource_state,
            Some(ResourceState::new(
                "Degraded",
                Some("Pod web/web-5884: CrashLoopBackOff, 0/1 containers ready, 4 restarts")
            ))
        );
    }

    #[test]
    fn tree_without_degraded_nodes_keeps_the_provider_state() {
        let mut pipeline = progressing_pipeline();
        let before = pipeline.resource_state.clone();
        degrade_from_tree(
            &mut pipeline,
            &tree(
                r#"[{"kind":"Pod","namespace":"web","name":"web-1","health":{"status":"Progressing"}},{"kind":"Pod","name":"web-2"}]"#,
            ),
        );

        assert_eq!(pipeline.status, PipelineStatus::Running);
        assert_eq!(pipeline.resource_state, before);
    }
}
