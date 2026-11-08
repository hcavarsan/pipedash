use super::catalog::map_available_pipeline;
use super::runs::{
    map_history_to_run,
    map_runs,
};
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

    let pipeline = map_application_to_pipeline(&app, "https://argocd.acme.dev");
    assert_eq!(pipeline.repository, "acme/web");
    assert_eq!(pipeline.branch.as_deref(), Some("release"));
    assert_eq!(pipeline.workflow_file.as_deref(), Some("deploy"));

    let history = &app.status.history.as_ref().unwrap()[0];
    let run = map_history_to_run(history, &app, "https://argocd.acme.dev");
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

    let ids: Vec<_> = map_runs(&app, "https://argocd.acme.dev", 10)
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

    let pipeline = map_application_to_pipeline(&app, "https://argocd.acme.dev");
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
    sync: &str, health: &str, health_message: Option<&str>, operation: Option<(&str, Option<&str>)>,
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

    let pipeline = map_application_to_pipeline(&syncing, "https://argocd.acme.dev");
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

fn tree(nodes: &str) -> types::logs::ApplicationTree {
    serde_json::from_str(&format!(r#"{{"nodes":{nodes}}}"#)).unwrap()
}

fn progressing_pipeline() -> Pipeline {
    let app = state_app("Synced", "Progressing", Some("waiting"), None);
    map_application_to_pipeline(&app, "https://argocd.acme.dev")
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
            Some("Pod web/web-7f9c-abc: back-off 40s restarting failed container=app (+2 more)")
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
