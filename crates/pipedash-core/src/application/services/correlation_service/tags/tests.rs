use chrono::Utc;
use pipedash_plugin_api::{
    CommitInfo,
    CommitSha,
};

use crate::application::services::correlation_service::tests::{
    artifact,
    digest,
    source,
    unit,
    Fx,
};
use crate::application::services::correlation_service::vocab::TAG_SHA;
use crate::application::test_support::StubProvider;
use crate::domain::Pipeline;

fn tagged(provider: i64) -> Pipeline {
    let mut api = unit(provider, "api");
    api.correlation.sources = vec![source("git.test/shop/api", &[], None)];
    api.correlation.consumed = vec![pipedash_plugin_api::ConsumedArtifact {
        artifact: artifact(
            "registry.test/shop/api",
            Some(digest('a')),
            Some("v1-abcdef1"),
        ),
        target: None,
    }];
    api
}

fn info() -> CommitInfo {
    let sha = CommitSha::parse(&format!("abcdef1{}", "0".repeat(33))).unwrap();
    CommitInfo {
        sha,
        authored_at: Utc::now(),
        committed_at: Utc::now(),
        message: "change".into(),
        parents: Vec::new(),
    }
}

#[tokio::test]
async fn ambiguous_tag_creates_no_edge() {
    let fx = Fx::new().await;
    fx.h.add_stub(|id| StubProvider::new(id).with_source_hosts(&["git.test"]))
        .await;
    let gitops = fx.provider("gitops").await;
    fx.apply(gitops, vec![tagged(gitops)]).await;
    fx.settle().await;
    assert!(fx.edges(TAG_SHA).await.is_empty());

    let fx = Fx::new().await;
    fx.h.add_stub(|id| {
        StubProvider::new(id)
            .with_source_hosts(&["git.test"])
            .with_commit(info())
    })
    .await;
    let gitops = fx.provider("gitops").await;
    fx.apply(gitops, vec![tagged(gitops)]).await;
    fx.settle().await;
    let edges = fx.edges(TAG_SHA).await;
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].confidence, "suggested");
}
