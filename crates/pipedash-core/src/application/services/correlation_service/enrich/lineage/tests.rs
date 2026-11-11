use std::sync::atomic::Ordering;

use chrono::Utc;
use pipedash_plugin_api::{
    ChangedFile,
    RegistryAuth,
};

use super::super::super::vocab::{
    commit_node_id,
    COMMIT,
    CONFIG_LINEAGE,
};
use super::super::tests::{
    digest,
    digest_of,
    registry,
    registry_after,
    sha,
    Fixture,
};
use super::added_references;
use crate::application::test_support::Gate;
use crate::infrastructure::database::PIPELINE_LEVEL;

const CONFIG_REPO: &str = "git.test/shop/config";

fn patch(text: &str) -> Vec<ChangedFile> {
    vec![ChangedFile {
        path: "deploy.yaml".into(),
        patch: Some(text.to_string()),
    }]
}

#[test]
fn only_added_lines_name_references() {
    let pinned = digest('a');
    let text = format!(
        "--- a/deploy.yaml\n+++ b/deploy.yaml\n@@ -1,3 +1,4 @@\n-  image: registry.test/shop/api:old\n+  image: registry.test/shop/api@{pinned}\n+  newTag: v1.2.3\n+  digest: {pinned}\n context: registry.test/keep/me:1\n+  url: registry.test/shop/web:stable\n"
    );
    let found = added_references(&patch(&text));
    assert!(found.contains(&format!("registry.test/shop/api@{pinned}")));
    assert!(found.contains("v1.2.3"));
    assert!(found.contains(&pinned));
    assert!(found.contains("registry.test/shop/web:stable"));
    assert!(!found.contains("registry.test/shop/api:old"));
    assert!(!found.contains("registry.test/keep/me:1"));
    assert!(!found.contains("b/deploy.yaml"));
}

#[test]
fn files_without_a_patch_name_nothing() {
    let files = vec![ChangedFile {
        path: "big.bin".into(),
        patch: None,
    }];
    assert!(added_references(&files).is_empty());
}

async fn unit_at_config_commit(f: &Fixture, provider: i64, image: &str) -> String {
    let subject = format!("stub__{provider}__api-prod");
    f.consume(
        provider,
        &subject,
        image,
        "registry.test/shop/api",
        Utc::now(),
    )
    .await;
    f.source(
        provider,
        &subject,
        PIPELINE_LEVEL,
        CONFIG_REPO,
        Some(&sha('c')),
    )
    .await;
    subject
}

#[tokio::test]
async fn config_commit_links_only_to_the_digest_it_introduced() {
    let f = Fixture::new().await;
    let running = digest('d');
    let (provider, stub) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_changes(patch(&format!(
                "+  image: registry.test/shop/api@{running}\n+  image: registry.test/shop/web@{}\n",
                digest('e')
            )))
        })
        .await;
    let subject = unit_at_config_commit(&f, provider, &running).await;

    let touched = f.service.follow_lineage().await.unwrap();

    assert!(touched.contains(&subject));
    let node = commit_node_id(CONFIG_REPO, &sha('c'));
    let edges = f
        .service
        .store
        .conn()
        .list_edges_from(COMMIT, &node)
        .await
        .unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].rule, CONFIG_LINEAGE);
    assert_eq!(edges[0].to_id, running);
    assert!(f.service.follow_lineage().await.unwrap().is_empty());
    assert_eq!(stub.change_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_unrelated_digest_in_config_creates_no_edge() {
    let f = Fixture::new().await;
    let (provider, _) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_changes(patch(&format!(
                    "+  image: registry.test/shop/api@{}\n",
                    digest('e')
                )))
        })
        .await;
    unit_at_config_commit(&f, provider, &digest('d')).await;

    assert!(f.service.follow_lineage().await.unwrap().is_empty());

    let node = commit_node_id(CONFIG_REPO, &sha('c'));
    assert!(f
        .service
        .store
        .conn()
        .list_edges_from(COMMIT, &node)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_tag_resolving_to_the_running_digest_links() {
    let f = Fixture::new().await;
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": digest('c'),
            "size": 2,
        },
        "layers": [],
    })
    .to_string()
    .into_bytes();
    let running = digest_of(&manifest);
    let host = registry(vec![("/v2/shop/api/manifests/v1".to_string(), manifest)]).await;
    let repository = format!("{host}/shop/api");
    let (provider, _) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_changes(patch(&format!("+  image: {repository}:v1\n")))
        })
        .await;
    let subject = format!("stub__{provider}__api-prod");
    f.consume(provider, &subject, &running, &repository, Utc::now())
        .await;
    f.source(
        provider,
        &subject,
        PIPELINE_LEVEL,
        CONFIG_REPO,
        Some(&sha('c')),
    )
    .await;

    assert!(f.service.follow_lineage().await.unwrap().contains(&subject));

    let node = commit_node_id(CONFIG_REPO, &sha('c'));
    let edges = f
        .service
        .store
        .conn()
        .list_edges_from(COMMIT, &node)
        .await
        .unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_id, running);
}

#[tokio::test]
async fn a_superseded_changes_response_writes_no_lineage_edge() {
    let f = Fixture::new().await;
    let running = digest('d');
    let gate = Gate::new();
    let (provider, _) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_changes(patch(&format!(
                    "+  image: registry.test/shop/api@{running}\n"
                )))
                .source_gated(&gate)
        })
        .await;
    unit_at_config_commit(&f, provider, &running).await;
    let service = f.service.clone();
    let pass = tokio::spawn(async move { service.follow_lineage().await });
    gate.arrived().await;
    f.h.provider_service
        .remove_provider(provider)
        .await
        .unwrap();
    gate.open();

    assert!(pass.await.unwrap().unwrap().is_empty());

    let node = commit_node_id(CONFIG_REPO, &sha('c'));
    assert!(f
        .service
        .store
        .conn()
        .list_edges_from(COMMIT, &node)
        .await
        .unwrap()
        .is_empty());
}

async fn tag_edges_with_credentials(supersede: bool) -> usize {
    let f = Fixture::new().await;
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": digest('c'),
            "size": 2,
        },
        "layers": [],
    })
    .to_string()
    .into_bytes();
    let running = digest_of(&manifest);
    let (login, _) = f
        .stub(|stub| {
            stub.with_registry_auth(RegistryAuth {
                username: None,
                secret: "token".into(),
                bearer: true,
            })
        })
        .await;
    let removing = supersede.then(|| (f.h.provider_service.clone(), login));
    let host = registry_after(
        vec![("/v2/shop/api/manifests/v1".to_string(), manifest)],
        removing,
    )
    .await;
    let repository = format!("{host}/shop/api");
    let (provider, _) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_changes(patch(&format!("+  image: {repository}:v1\n")))
        })
        .await;
    let subject = format!("stub__{provider}__api-prod");
    f.consume(provider, &subject, &running, &repository, Utc::now())
        .await;
    f.source(
        provider,
        &subject,
        PIPELINE_LEVEL,
        CONFIG_REPO,
        Some(&sha('c')),
    )
    .await;

    f.service.follow_lineage().await.unwrap();

    let node = commit_node_id(CONFIG_REPO, &sha('c'));
    f.service
        .store
        .conn()
        .list_edges_from(COMMIT, &node)
        .await
        .unwrap()
        .len()
}

#[tokio::test]
async fn a_tag_lookup_with_superseded_credentials_writes_no_edge() {
    assert_eq!(tag_edges_with_credentials(false).await, 1);
    assert_eq!(tag_edges_with_credentials(true).await, 0);
}
