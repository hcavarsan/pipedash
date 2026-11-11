use super::*;

const APP_OK: &str = r#"{"metadata":{"name":"ok"},"spec":{"source":{"repoURL":"https://github.com/acme/ok","path":"k8s","targetRevision":"main"},"destination":{"server":"https://kubernetes.default.svc","namespace":"ok"},"project":"default"},"status":{"sync":{"status":"Synced"},"health":{"status":"Healthy"}}}"#;

fn parse_list_with(app: &str) -> Vec<String> {
    let list: ApplicationList =
        serde_json::from_str(&format!(r#"{{"metadata":{{}},"items":[{APP_OK},{app}]}}"#)).unwrap();
    list.items.into_iter().map(|a| a.metadata.name).collect()
}

#[test]
fn list_keeps_app_with_destination_name() {
    let app = r#"{"metadata":{"name":"by-name"},"spec":{"source":{"repoURL":"https://github.com/acme/a","path":"k8s","targetRevision":"main"},"destination":{"name":"in-cluster","namespace":"a"},"project":"default"},"status":{"sync":{"status":"Synced"},"health":{"status":"Healthy"}}}"#;
    assert_eq!(parse_list_with(app), ["ok", "by-name"]);
}

#[test]
fn list_keeps_app_without_target_revision_or_namespace() {
    let app = r#"{"metadata":{"name":"head"},"spec":{"source":{"repoURL":"https://github.com/acme/b","path":"k8s"},"destination":{"server":"https://kubernetes.default.svc"},"project":"default"},"status":{"sync":{"status":"Synced"},"health":{"status":"Healthy"}}}"#;
    assert_eq!(parse_list_with(app), ["ok", "head"]);
}

#[test]
fn list_keeps_multi_source_app_with_history() {
    let app = r#"{"metadata":{"name":"multi"},"spec":{"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"},{"repoURL":"https://charts.acme.dev","chart":"redis","targetRevision":"18.1.0"}],"destination":{"server":"https://kubernetes.default.svc","namespace":"c"},"project":"default"},"status":{"sync":{"status":"Synced","comparedTo":{"source":{"repoURL":""},"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"}],"destination":{"server":"https://kubernetes.default.svc","namespace":"c"}},"revisions":["abc123","18.1.0"]},"health":{"status":"Healthy"},"history":[{"deployedAt":"2026-10-20T10:00:00Z","id":0,"source":{"repoURL":""},"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"}],"revisions":["abc123","18.1.0"]}]}}"#;
    assert_eq!(parse_list_with(app), ["ok", "multi"]);
}

#[test]
fn list_keeps_app_awaiting_first_reconcile() {
    let app = r#"{"metadata":{"name":"new"},"spec":{"source":{"repoURL":"https://github.com/acme/d","path":"k8s","targetRevision":"main"},"destination":{"server":"https://kubernetes.default.svc","namespace":"d"},"project":"default"},"status":{"sync":{"status":"","comparedTo":{"source":{"repoURL":""},"destination":{}}},"health":{}}}"#;
    assert_eq!(parse_list_with(app), ["ok", "new"]);
}

#[test]
fn primary_source_prefers_sources_over_empty_source() {
    let spec: ApplicationSpec = serde_json::from_str(
        r#"{"source":{"repoURL":""},"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"}],"destination":{"server":"https://kubernetes.default.svc"},"project":"default"}"#,
    )
    .unwrap();
    assert_eq!(spec.primary_source().repo_url, "https://github.com/acme/c");
}

#[test]
fn application_without_status_is_pending_and_unknown() {
    let app: Application = serde_json::from_str(
        r#"{"metadata":{"name":"fresh"},"spec":{"source":{"repoURL":"https://github.com/acme/e"},"destination":{"name":"in-cluster"},"project":"default"}}"#,
    )
    .unwrap();

    assert_eq!(app.status.health.status, "");
    assert_eq!(app.status.sync.status, "");
    assert!(app.status.resources.is_empty());
}
