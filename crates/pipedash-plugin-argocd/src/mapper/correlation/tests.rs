use pipedash_plugin_api::{
    ClusterId,
    CommitSha,
    Correlation,
    PathScope,
    RepoId,
    SourceObservation,
    TargetLifetime,
    TargetObservation,
    UnitId,
};
use serde_json::{
    json,
    Value,
};

use super::{
    history,
    pipeline,
};
use crate::types::logs::Pod;
use crate::types::{
    Application,
    RevisionHistory,
};

fn sha(fill: char) -> String {
    fill.to_string().repeat(40)
}

fn digest(fill: char) -> String {
    format!("sha256:{}", fill.to_string().repeat(64))
}

fn app(metadata: Value, spec: Value, status: Value) -> Application {
    let mut metadata = metadata;
    metadata["name"] = json!("web");
    metadata["namespace"] = json!("argocd");
    serde_json::from_value(json!({"metadata": metadata, "spec": spec, "status": status})).unwrap()
}

fn remote_app(status: Value) -> Application {
    app(
        json!({}),
        json!({
            "source": {"repoURL": "https://github.com/acme/web", "path": "deploy"},
            "destination": {"server": "https://prod.example.com:6443", "namespace": "web"},
            "project": "default"
        }),
        status,
    )
}

fn in_cluster_app(destination: Value, status: Value) -> Application {
    app(
        json!({}),
        json!({
            "source": {"repoURL": "https://github.com/acme/web"},
            "destination": destination,
            "project": "default"
        }),
        status,
    )
}

fn source(repo: &str, scopes: &[&str], commit: Option<String>) -> SourceObservation {
    SourceObservation {
        repo: RepoId::parse(repo).unwrap(),
        path_scopes: scopes.iter().map(|scope| PathScope::new(scope)).collect(),
        commit: commit.as_deref().and_then(CommitSha::parse),
    }
}

fn pod(containers: Value) -> Pod {
    serde_json::from_value(json!({
        "metadata": {"name": "web-1", "namespace": "web"},
        "status": {"containerStatuses": containers}
    }))
    .unwrap()
}

type Consumed = (String, Option<String>, Option<String>, Option<usize>);

fn consumed(correlation: &Correlation) -> Vec<Consumed> {
    correlation
        .consumed
        .iter()
        .map(|consumed| {
            let artifact = &consumed.artifact;
            (
                artifact.repository.as_str().to_string(),
                artifact.tag.clone(),
                artifact
                    .digest
                    .as_ref()
                    .map(|digest| digest.as_str().to_string()),
                consumed.target,
            )
        })
        .collect()
}

fn entry(value: Value) -> RevisionHistory {
    let mut value = value;
    value["id"] = json!(3);
    value["deployedAt"] = json!("2026-10-20T10:00:30Z");
    serde_json::from_value(value).unwrap()
}

#[test]
fn two_sources_bind_their_own_revisions() {
    let app = app(
        json!({}),
        json!({
            "sources": [
                {"repoURL": "https://github.com/acme/web", "path": "deploy"},
                {"repoURL": "git@github.com:acme/config.git", "path": "./apps/web/"}
            ],
            "destination": {"server": "https://prod.example.com:6443", "namespace": "web"},
            "project": "default"
        }),
        json!({"sync": {"status": "Synced", "revisions": [sha('a'), sha('b')]}}),
    );

    assert_eq!(
        pipeline(&app, None, None).sources,
        [
            source("https://github.com/acme/web", &["deploy"], Some(sha('a'))),
            source(
                "https://github.com/acme/config",
                &["apps/web"],
                Some(sha('b'))
            ),
        ]
    );
}

#[test]
fn chart_sources_are_skipped_and_keep_the_revision_positions() {
    let app = app(
        json!({}),
        json!({
            "sources": [
                {"repoURL": "https://charts.example.com", "chart": "redis", "targetRevision": "18.1.0"},
                {"repoURL": "https://github.com/acme/values", "path": "redis"}
            ],
            "destination": {"server": "https://prod.example.com:6443"},
            "project": "default"
        }),
        json!({"sync": {"status": "Synced", "revisions": ["18.1.0", sha('c')]}}),
    );

    assert_eq!(
        pipeline(&app, None, None).sources,
        [source(
            "https://github.com/acme/values",
            &["redis"],
            Some(sha('c'))
        )]
    );
}

#[test]
fn single_source_takes_the_sync_revision_only_when_it_is_a_commit() {
    let commit = remote_app(json!({"sync": {"status": "Synced", "revision": sha('d')}}));
    let branch = remote_app(json!({"sync": {"status": "Synced", "revision": "HEAD"}}));
    let none = remote_app(json!({"sync": {"status": "Synced"}}));

    assert_eq!(
        pipeline(&commit, None, None).sources,
        [source(
            "https://github.com/acme/web",
            &["deploy"],
            Some(sha('d'))
        )]
    );
    assert_eq!(
        pipeline(&branch, None, None).sources,
        [source("https://github.com/acme/web", &["deploy"], None)]
    );
    assert_eq!(
        pipeline(&none, None, None).sources,
        [source("https://github.com/acme/web", &["deploy"], None)]
    );
}

#[test]
fn source_without_a_git_repository_is_not_observed() {
    let app = app(
        json!({}),
        json!({
            "source": {"repoURL": "oci://registry.example.com/charts/web"},
            "destination": {"server": "https://prod.example.com:6443"},
            "project": "default"
        }),
        json!({}),
    );

    assert!(pipeline(&app, None, None).sources.is_empty());
}

#[test]
fn unit_parent_label_and_target_come_from_the_application() {
    let app = app(
        json!({
            "ownerReferences": [
                {"kind": "ConfigMap", "name": "other"},
                {"kind": "ApplicationSet", "name": "shop"}
            ],
            "labels": {
                "app.kubernetes.io/name": "web",
                "app.kubernetes.io/part-of": "shop-suite"
            }
        }),
        json!({
            "source": {"repoURL": "https://github.com/acme/web"},
            "destination": {"server": "https://prod.example.com:6443", "namespace": "web"},
            "project": "default"
        }),
        json!({}),
    );

    let correlation = pipeline(&app, None, None);

    assert_eq!(correlation.unit, UnitId::new("argocd/web"));
    assert_eq!(correlation.unit_parent, UnitId::new("appset/shop"));
    assert_eq!(correlation.app_label.as_deref(), Some("shop-suite"));
    assert_eq!(
        correlation.targets,
        [TargetObservation {
            cluster: ClusterId::from_server("https://prod.example.com:6443").unwrap(),
            namespace: Some("web".to_string()),
            name: "https://prod.example.com:6443/web".to_string(),
            lifetime: TargetLifetime::Standing,
        }]
    );
}

#[test]
fn label_falls_back_to_the_name_and_unowned_apps_have_no_parent() {
    let named = app(
        json!({"labels": {"app.kubernetes.io/name": "web", "app.kubernetes.io/part-of": " "}}),
        json!({"destination": {}, "project": "default"}),
        json!({}),
    );
    let bare = remote_app(json!({}));

    let correlation = pipeline(&named, None, None);
    assert_eq!(correlation.app_label.as_deref(), Some("web"));
    assert_eq!(correlation.unit_parent, None);
    assert!(correlation.sources.is_empty());
    assert!(correlation.targets.is_empty());

    let correlation = pipeline(&bare, None, None);
    assert_eq!(correlation.app_label, None);
    assert_eq!(correlation.unit_parent, None);
}

#[test]
fn in_cluster_target_is_identified_by_the_kube_system_uid() {
    let status = json!({});
    let cases = [
        json!({"server": "https://kubernetes.default.svc", "namespace": "web"}),
        json!({"name": "in-cluster", "namespace": "web"}),
    ];

    for destination in cases {
        let app = in_cluster_app(destination, status.clone());

        let known = pipeline(&app, Some("KUBE-UID"), None);
        assert_eq!(known.targets.len(), 1);
        assert_eq!(
            known.targets[0].cluster,
            ClusterId::from_uid("kube-uid").unwrap()
        );
        assert_eq!(known.targets[0].namespace.as_deref(), Some("web"));

        assert!(pipeline(&app, None, None).targets.is_empty());
    }
}

#[test]
fn destination_name_without_server_is_a_host_target() {
    let app = in_cluster_app(json!({"name": "prod-eu"}), json!({}));

    let target = &pipeline(&app, Some("kube-uid"), None).targets[0];

    assert_eq!(target.cluster, ClusterId::from_host("prod-eu").unwrap());
    assert_eq!(target.namespace, None);
    assert_eq!(target.name, "prod-eu");
}

#[test]
fn pods_are_consumed_sorted_and_deduplicated_with_their_pulled_digest() {
    let app = in_cluster_app(
        json!({"server": "https://kubernetes.default.svc", "namespace": "web"}),
        json!({}),
    );
    let pods = vec![
        pod(json!([
            {"name": "app", "image": "registry.test/shop/web:2",
                "imageID": format!("docker-pullable://registry.test/shop/web@{}", digest('2'))},
            {"name": "mesh", "image": "ghcr.io/acme/sidecar:3"}
        ])),
        pod(json!([
            {"name": "app", "image": "registry.test/shop/api:1.2",
                "imageID": format!("registry.test/shop/api@{}", digest('1'))}
        ])),
        pod(json!([
            {"name": "app", "image": "registry.test/shop/api:1.2",
                "imageID": format!("registry.test/shop/api@{}", digest('1'))},
            {"name": "empty"}
        ])),
    ];

    let correlation = pipeline(&app, Some("kube-uid"), Some(pods.as_slice()));

    assert_eq!(
        consumed(&correlation),
        [
            (
                "ghcr.io/acme/sidecar".to_string(),
                Some("3".to_string()),
                None,
                Some(0)
            ),
            (
                "registry.test/shop/api".to_string(),
                Some("1.2".to_string()),
                Some(digest('1')),
                Some(0)
            ),
            (
                "registry.test/shop/web".to_string(),
                Some("2".to_string()),
                Some(digest('2')),
                Some(0)
            ),
        ]
    );
}

#[test]
fn consumed_artifacts_have_no_target_when_the_cluster_is_unknown() {
    let app = in_cluster_app(json!({"name": "in-cluster"}), json!({}));
    let pods = [pod(
        json!([{"name": "app", "image": "registry.test/shop/api:1"}]),
    )];

    let correlation = pipeline(&app, None, Some(pods.as_slice()));

    assert!(correlation.targets.is_empty());
    assert_eq!(
        consumed(&correlation),
        [(
            "registry.test/shop/api".to_string(),
            Some("1".to_string()),
            None,
            None
        )]
    );
}

#[test]
fn summary_images_are_consumed_without_pods_and_carry_a_digest_only_when_the_ref_does() {
    let app = remote_app(json!({"summary": {"images": [
        "ghcr.io/acme/web:1.0",
        format!("ghcr.io/acme/web@{}", digest('9')),
        "not an image"
    ]}}));

    let correlation = pipeline(&app, None, None);

    assert_eq!(
        consumed(&correlation),
        [
            (
                "ghcr.io/acme/web".to_string(),
                Some("1.0".to_string()),
                None,
                Some(0)
            ),
            (
                "ghcr.io/acme/web".to_string(),
                None,
                Some(digest('9')),
                Some(0)
            ),
        ]
    );
}

#[test]
fn history_sources_bind_to_the_history_revisions() {
    let entry = entry(json!({
        "source": {"repoURL": ""},
        "sources": [
            {"repoURL": "https://github.com/acme/web", "path": "deploy"},
            {"repoURL": "https://github.com/acme/config"}
        ],
        "revisions": [sha('a'), sha('b')]
    }));

    assert_eq!(
        history(&entry).sources,
        [
            source("https://github.com/acme/web", &["deploy"], Some(sha('a'))),
            source("https://github.com/acme/config", &[], Some(sha('b'))),
        ]
    );
}

#[test]
fn history_single_source_binds_its_revision() {
    let entry = entry(json!({
        "source": {"repoURL": "https://github.com/acme/web", "path": "deploy"},
        "revision": sha('e')
    }));

    let correlation = history(&entry);

    assert_eq!(
        correlation.sources,
        [source(
            "https://github.com/acme/web",
            &["deploy"],
            Some(sha('e'))
        )]
    );
    assert!(correlation.targets.is_empty() && correlation.consumed.is_empty());
}

#[test]
fn history_without_a_recorded_source_observes_none() {
    let entry = entry(json!({"source": {"repoURL": ""}, "revision": sha('f')}));

    assert!(history(&entry).is_empty());
}
