use std::sync::atomic::Ordering;

use chrono::Utc;
use pipedash_plugin_api::{
    CommitSha,
    Provenance,
    RepoId,
    SourceObservation,
};

use super::super::super::vocab::{
    commit_node_id,
    ARTIFACT,
    ATTESTATION_COMMIT,
    ORIGIN_ATTESTATION,
    REL_PRODUCED,
};
use super::super::tests::{
    digest,
    digest_of,
    registry,
    sha,
    Fixture,
};
use super::names_run;
use crate::infrastructure::PipedashConfig;

#[test]
fn an_invocation_names_a_run_only_at_a_path_boundary() {
    let cases = [
        ("https://ci.test/runs/9", "https://ci.test/runs/9", true),
        (
            "https://ci.test/runs/9/attempts/1",
            "https://ci.test/runs/9",
            true,
        ),
        ("https://ci.test/runs/90", "https://ci.test/runs/9", false),
        ("https://ci.test/runs/9", "", false),
        ("https://ci.test/runs/8", "https://ci.test/runs/9", false),
    ];
    for (invocation, logs_url, expected) in cases {
        assert_eq!(names_run(invocation, logs_url), expected, "{invocation}");
    }
}

#[tokio::test]
async fn image_labels_are_stored_on_the_artifact() {
    let f = Fixture::new().await;
    let (provider, _) = f.stub(|stub| stub).await;
    let config = serde_json::json!({
        "config": { "Labels": {
            "org.opencontainers.image.source": "https://git.test/shop/api.git",
            "org.opencontainers.image.revision": sha('e'),
        }}
    })
    .to_string()
    .into_bytes();
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": digest_of(&config),
            "size": config.len(),
        },
        "layers": [],
    })
    .to_string()
    .into_bytes();
    let image = digest_of(&manifest);
    let host = registry(vec![
        (format!("/v2/shop/api/manifests/{image}"), manifest),
        (format!("/v2/shop/api/blobs/{}", digest_of(&config)), config),
    ])
    .await;
    let repository = format!("{host}/shop/api");
    f.consume(provider, "stub__1__api", &image, &repository, Utc::now())
        .await;

    let touched = f.service.read_registries().await.unwrap();

    assert!(touched.contains("stub__1__api"));
    let stored = f
        .service
        .store
        .conn()
        .get_artifact(&image)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.label_source.as_deref(), Some("git.test/shop/api"));
    assert_eq!(stored.label_revision, Some(sha('e')));
    assert!(stored.registry_checked_at.is_some());
}

#[tokio::test]
async fn provider_provenance_attests_the_producing_run() {
    let f = Fixture::new().await;
    let commit = CommitSha::parse(&sha('b')).unwrap();
    let repo = RepoId::parse("https://git.test/shop/api").unwrap();
    let build = |id: i64| format!("stub__{id}__build");
    let (provider, stub) = f
        .stub(|stub| {
            let pipeline = build(stub.provider_id);
            stub.with_source_hosts(&["git.test"])
                .with_provenance(Provenance {
                    pipeline_id: Some(pipeline),
                    run_number: Some(5),
                    source: SourceObservation {
                        repo: repo.clone(),
                        path_scopes: Vec::new(),
                        commit: Some(commit.clone()),
                    },
                })
        })
        .await;
    let host = registry(Vec::new()).await;
    let repository = format!("{host}/shop/api");
    let image = digest('d');
    f.consume(provider, "stub__1__api", &image, &repository, Utc::now())
        .await;
    let pipeline = build(provider);
    f.source(provider, &pipeline, 5, repo.as_str(), Some(commit.as_str()))
        .await;
    f.concluded_run(&pipeline, 5, Utc::now()).await;

    let touched = f.service.read_registries().await.unwrap();

    assert!(touched.contains(&pipeline));
    let mut conn = f.service.store.conn();
    let produced: Vec<_> = conn
        .list_artifact_observations_by_subject(&pipeline)
        .await
        .unwrap();
    assert_eq!(produced.len(), 1);
    assert_eq!(produced[0].relation, REL_PRODUCED);
    assert_eq!(produced[0].origin, ORIGIN_ATTESTATION);
    assert_eq!(produced[0].run_number, 5);
    let edges = conn.list_edges_from(ARTIFACT, &image).await.unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].rule, ATTESTATION_COMMIT);
    assert_eq!(
        edges[0].to_id,
        commit_node_id(repo.as_str(), commit.as_str())
    );
    assert_eq!(stub.provenance_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn registry_read_false_skips_registry_work() {
    let mut config = PipedashConfig::default();
    config.correlation.registry_read = false;
    let f = Fixture::with_config(config).await;
    let (provider, stub) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_provenance(Provenance {
                    pipeline_id: None,
                    run_number: None,
                    source: SourceObservation {
                        repo: RepoId::parse("https://git.test/shop/api").unwrap(),
                        path_scopes: Vec::new(),
                        commit: None,
                    },
                })
        })
        .await;
    let image = digest('d');
    f.consume(
        provider,
        "stub__1__api",
        &image,
        "127.0.0.1:9/shop/api",
        Utc::now(),
    )
    .await;

    assert!(f
        .service
        .read_registries()
        .await
        .unwrap()
        .contains("stub__1__api"));
    assert!(f.service.read_registries().await.unwrap().is_empty());

    assert_eq!(stub.provenance_calls.load(Ordering::SeqCst), 1);
    let stored = f
        .service
        .store
        .conn()
        .get_artifact(&image)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.registry_checked_at.is_none());
}
