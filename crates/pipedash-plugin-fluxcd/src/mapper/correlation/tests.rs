use pipedash_plugin_api::{
    Digest,
    ImageRef,
};
use serde_json::{
    json,
    Value,
};

use super::*;
use crate::mapper::build_runs;

const COMMIT: &str = "6b7aab8a1f0c4e5d9a3b2c1d0e9f8a7b6c5d4e3f";
const DIGEST: &str = "sha256:0f5a7c1e9b3d2468ace02468ace02468ace02468ace02468ace02468ace02460";

fn resource(kind: Kind, labels: Value, spec: Value, status: Value) -> FluxResource {
    FluxResource::from_value(
        kind,
        json!({
            "metadata": {
                "name": "shop",
                "namespace": "flux-system",
                "generation": 1,
                "creationTimestamp": "2026-10-20T10:00:00Z",
                "labels": labels,
            },
            "spec": spec,
            "status": status,
        }),
    )
    .unwrap()
}

fn ready(revision: &str) -> Value {
    json!({
        "type": "Ready",
        "status": "True",
        "reason": "ReconciliationSucceeded",
        "message": format!("Applied revision: {revision}"),
        "observedGeneration": 1,
        "lastTransitionTime": "2026-10-21T10:00:05Z",
    })
}

fn image() -> ArtifactObservation {
    let mut image = ImageRef::parse("registry.example.test/shop/api:1.4.2")
        .unwrap()
        .into_observation();
    image.digest = Digest::parse(DIGEST);

    image
}

fn facts() -> Facts {
    Facts {
        repo: RepoId::parse("ssh://git@git.example.test/acme/fleet.git"),
        cluster: ClusterId::from_uid("5C1F9F1E"),
        images: vec![image()],
    }
}

fn kustomization() -> FluxResource {
    let revision = format!("main@sha1:{COMMIT}");

    resource(
        Kind::Kustomization,
        json!({ "app.kubernetes.io/name": "api", "app.kubernetes.io/part-of": "shop" }),
        json!({
            "path": "./apps/shop/",
            "sourceRef": { "kind": "GitRepository", "name": "fleet" },
            "targetNamespace": "shop",
            "dependsOn": [
                { "name": "infra" },
                { "name": "certs", "namespace": "security" },
                { "name": "" },
            ],
        }),
        json!({
            "observedGeneration": 1,
            "lastAttemptedRevision": revision,
            "conditions": [ready(&revision)],
        }),
    )
}

#[test]
fn kustomization_declares_its_unit_dependencies_label_source_and_one_target() {
    let correlation = pipeline(&kustomization(), &facts());

    assert_eq!(
        correlation,
        Correlation {
            unit: UnitId::new("Kustomization/flux-system/shop"),
            unit_depends_on: vec![
                UnitId::new("Kustomization/flux-system/infra").unwrap(),
                UnitId::new("Kustomization/security/certs").unwrap(),
            ],
            app_label: Some("shop".to_string()),
            sources: vec![SourceObservation {
                repo: RepoId::from_host_path("git.example.test", "acme/fleet").unwrap(),
                path_scopes: vec![PathScope::new("apps/shop")],
                commit: CommitSha::parse(COMMIT),
            }],
            targets: vec![TargetObservation {
                cluster: ClusterId::from_uid("5c1f9f1e").unwrap(),
                namespace: Some("shop".to_string()),
                name: "shop".to_string(),
                lifetime: TargetLifetime::Standing,
            }],
            consumed: vec![ConsumedArtifact {
                artifact: image(),
                target: Some(0),
            }],
            ..Correlation::default()
        }
    );
}

#[test]
fn the_app_label_falls_back_to_the_name_label_and_skips_empty_values() {
    let labelled = |labels: Value| {
        pipeline(
            &resource(Kind::Kustomization, labels, json!({}), json!({})),
            &Facts::default(),
        )
        .app_label
    };

    assert_eq!(
        labelled(json!({ "app.kubernetes.io/name": "api" })),
        Some("api".to_string())
    );
    assert_eq!(
        labelled(json!({ "app.kubernetes.io/part-of": "", "app.kubernetes.io/name": "api" })),
        Some("api".to_string())
    );
    assert_eq!(labelled(json!({})), None);
}

#[test]
fn only_git_repository_sources_have_a_git_source_reference() {
    assert_eq!(
        git_source(&kustomization()).map(|reference| reference.name.as_str()),
        Some("fleet")
    );
    for kind in ["OCIRepository", "Bucket", "HelmRepository"] {
        let other = resource(
            Kind::Kustomization,
            json!({}),
            json!({ "sourceRef": { "kind": kind, "name": "fleet" } }),
            json!({}),
        );

        assert!(git_source(&other).is_none(), "{kind}");
    }
}

#[test]
fn without_a_repository_there_is_no_source_observation() {
    let facts = Facts {
        repo: None,
        ..facts()
    };

    let correlation = pipeline(&kustomization(), &facts);

    assert!(correlation.sources.is_empty());
    assert_eq!(correlation.targets.len(), 1);
}

#[test]
fn without_a_cluster_identity_there_is_no_target_and_no_consumed_image() {
    let facts = Facts {
        cluster: None,
        ..facts()
    };

    let correlation = pipeline(&kustomization(), &facts);

    assert!(correlation.targets.is_empty());
    assert!(correlation.consumed.is_empty());
    assert_eq!(correlation.sources.len(), 1);
}

#[test]
fn the_target_namespace_defaults_to_the_namespace_of_the_object() {
    let plain = resource(Kind::Kustomization, json!({}), json!({}), json!({}));

    let correlation = pipeline(&plain, &facts());

    assert_eq!(
        correlation.targets[0].namespace.as_deref(),
        Some("flux-system")
    );
    assert_eq!(correlation.targets[0].name, "flux-system");
}

#[test]
fn a_helm_release_scopes_the_chart_path_and_has_no_commit_in_its_chart_version() {
    let release = resource(
        Kind::HelmRelease,
        json!({}),
        json!({
            "targetNamespace": "shop",
            "dependsOn": [{ "name": "db" }],
            "chart": { "spec": {
                "chart": "./charts/shop",
                "sourceRef": { "kind": "GitRepository", "name": "fleet" },
            } },
        }),
        json!({ "lastAttemptedRevision": "0.1.0", "conditions": [ready("0.1.0")] }),
    );

    let correlation = pipeline(&release, &facts());

    assert_eq!(
        correlation.unit,
        UnitId::new("HelmRelease/flux-system/shop")
    );
    assert_eq!(
        correlation.unit_depends_on,
        vec![UnitId::new("HelmRelease/flux-system/db").unwrap()]
    );
    assert_eq!(
        correlation.sources[0].path_scopes,
        vec![PathScope::new("charts/shop")]
    );
    assert_eq!(correlation.sources[0].commit, None);
    assert_eq!(correlation.targets[0].namespace.as_deref(), Some("shop"));
}

#[test]
fn history_runs_carry_the_commit_of_their_own_revision() {
    let resource = resource(
        Kind::Kustomization,
        json!({}),
        json!({ "path": "apps/shop", "sourceRef": { "kind": "GitRepository", "name": "fleet" } }),
        json!({
            "observedGeneration": 1,
            "conditions": [ready("main")],
            "history": [
                {
                    "digest": "sha256:b",
                    "firstReconciled": "2026-10-21T10:00:00Z",
                    "lastReconciled": "2026-10-21T10:00:05Z",
                    "lastReconciledDuration": "5s",
                    "lastReconciledStatus": "ReconciliationSucceeded",
                    "metadata": { "revision": format!("main@sha1:{COMMIT}") },
                },
                {
                    "digest": "sha256:a",
                    "firstReconciled": "2026-10-20T10:00:00Z",
                    "lastReconciled": "2026-10-20T10:00:05Z",
                    "lastReconciledDuration": "5s",
                    "lastReconciledStatus": "ReconciliationSucceeded",
                    "metadata": { "revision": "main@sha1:abc" },
                },
            ],
        }),
    );
    let mut runs = build_runs(&resource, &[]).unwrap();

    attach_runs(&mut runs, &resource, &facts());

    assert_eq!(runs.len(), 2);
    assert_eq!(
        runs[0].correlation.sources,
        vec![SourceObservation {
            repo: facts().repo.unwrap(),
            path_scopes: vec![PathScope::new("apps/shop")],
            commit: CommitSha::parse(COMMIT),
        }]
    );
    assert!(runs[1].correlation.sources.is_empty());
}

#[test]
fn runs_have_no_source_without_a_repository() {
    let resource = kustomization();
    let mut runs = build_runs(&resource, &[]).unwrap();

    attach_runs(&mut runs, &resource, &Facts::default());

    assert!(runs.iter().all(|run| run.correlation.sources.is_empty()));
}
