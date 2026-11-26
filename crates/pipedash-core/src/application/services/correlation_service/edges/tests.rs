use crate::application::services::correlation_service::tests::{
    consumed,
    digest,
    source,
    target,
    unit,
    Fx,
};
use crate::application::services::correlation_service::vocab::{
    UNIT_CONSUMED,
    UNIT_SOURCE_COMMIT,
    UNIT_TARGET,
};

#[tokio::test]
async fn sources_do_not_cross_join_commits() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.sources = vec![
        source("git.test/a/one", &[], Some('1')),
        source("git.test/b/two", &[], Some('2')),
    ];
    fx.apply(gitops, vec![api]).await;

    let mut found: Vec<String> = fx
        .edges(UNIT_SOURCE_COMMIT)
        .await
        .into_iter()
        .map(|e| e.to_id)
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            format!("git.test/a/one@{}", "1".repeat(40)),
            format!("git.test/b/two@{}", "2".repeat(40)),
        ]
    );
}

#[tokio::test]
async fn targets_do_not_cross_join() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.targets = vec![target("1", "a", "a"), target("2", "b", "b")];
    fx.apply(gitops, vec![api]).await;

    let mut edges: Vec<String> = fx
        .edges(UNIT_TARGET)
        .await
        .into_iter()
        .map(|e| e.to_id)
        .collect();
    edges.sort();
    assert_eq!(edges, ["uid:1|a", "uid:2|b"]);
    let app = fx.app("api").await;
    let mut conn = fx.svc.store.conn();
    let rows = conn
        .list_application_targets_by_application(app.id)
        .await
        .unwrap();
    let keys: Vec<&str> = rows.iter().map(|r| r.target_key.as_str()).collect();
    assert_eq!(keys, ["uid:1|a", "uid:2|b"]);
}

#[tokio::test]
async fn digest_is_live_only_in_its_target() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.targets = vec![target("1", "a", "a"), target("2", "b", "b")];
    api.correlation.consumed = vec![consumed("registry.test/shop/api", digest('a'), Some(0))];
    fx.apply(gitops, vec![api]).await;

    let edges = fx.edges(UNIT_CONSUMED).await;
    assert_eq!(edges.len(), 1);
    let evidence: serde_json::Value = serde_json::from_str(&edges[0].evidence_json).unwrap();
    assert_eq!(evidence["targets"], serde_json::json!(["uid:1|a"]));
}
