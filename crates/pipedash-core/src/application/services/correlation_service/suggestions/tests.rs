use crate::application::services::correlation_service::tests::{
    consumed,
    digest,
    source,
    unit,
    Fx,
};
use crate::application::services::correlation_service::vocab::{
    SAME_ARTIFACT_REPO_DIGEST,
    SAME_LABEL,
    SAME_REPO_PATH,
};
use crate::domain::application::overrides::ApplicationEdit;

#[tokio::test]
async fn shared_config_repo_does_not_merge() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.sources = vec![source("git.test/shop/config", &["apps"], None)];
    let mut web = unit(gitops, "web");
    web.correlation.sources = vec![source("git.test/shop/config", &["apps"], None)];
    fx.apply(gitops, vec![api, web]).await;

    assert_eq!(fx.apps().await.len(), 2);
    assert_eq!(fx.edges(SAME_REPO_PATH).await.len(), 1);
}

#[tokio::test]
async fn shared_base_image_does_not_merge() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.consumed = vec![consumed("registry.test/base/node", digest('b'), None)];
    let mut web = unit(gitops, "web");
    web.correlation.consumed = vec![consumed("registry.test/base/node", digest('b'), None)];
    fx.apply(gitops, vec![api, web]).await;

    assert_eq!(fx.apps().await.len(), 2);
    assert_eq!(fx.edges(SAME_ARTIFACT_REPO_DIGEST).await.len(), 1);
}

#[tokio::test]
async fn same_label_without_app_ref_only_suggests() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.app_label = Some("shop".into());
    let mut web = unit(gitops, "web");
    web.correlation.app_label = Some("shop".into());
    fx.apply(gitops, vec![api, web]).await;

    assert_eq!(fx.apps().await.len(), 2);
    let open = fx.open().await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].dto.rule_count, 1);
}

#[tokio::test]
async fn rejected_suggestion_never_returns() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.app_label = Some("shop".into());
    let mut web = unit(gitops, "web");
    web.correlation.app_label = Some("shop".into());
    fx.apply(gitops, vec![api.clone(), web.clone()]).await;

    let suggestion = fx.open().await.remove(0).dto.id;
    let app = fx.app("api").await;
    fx.svc
        .edit_internal(
            app.id,
            ApplicationEdit::RejectSuggestion {
                suggestion_id: suggestion,
            },
            None,
        )
        .await
        .unwrap();
    assert!(fx.open().await.is_empty());

    fx.svc.resolve_all().await.unwrap();
    fx.apply(gitops, vec![api, web]).await;
    assert!(fx.open().await.is_empty());
    assert!(fx.edges(SAME_LABEL).await.is_empty());
}
