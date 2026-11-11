use chrono::Utc;

use crate::application::services::correlation_service::tests::{
    consumed,
    digest,
    sha,
    source,
    target,
    unit,
    Fx,
};
use crate::application::services::correlation_service::vocab::{
    commit_node_id,
    LABEL_UNCONFIRMED,
    SAME_ARTIFACT_REPO_DIGEST,
    SAME_LABEL,
    SAME_REPO_PATH,
    USER_CONFIRMED,
};
use crate::domain::application::dto::SuggestionKind;
use crate::domain::application::overrides::{
    ApplicationEdit,
    OverrideKind,
};
use crate::domain::Pipeline;
use crate::infrastructure::database::ArtifactRow;

const REPO: &str = "git.test/shop/api";
const IMAGE: &str = "registry.test/shop/api";

async fn labelled_unit(fx: &Fx) -> (i64, Pipeline) {
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.targets = vec![target("1", "shop", "shop")];
    api.correlation.consumed = vec![consumed(IMAGE, digest('a'), Some(0))];
    fx.apply(gitops, vec![api.clone()]).await;
    mark_label(fx).await;
    (gitops, api)
}

async fn mark_label(fx: &Fx) {
    fx.settle().await;
    fx.svc
        .store
        .conn()
        .mark_artifact_registry_checked(
            digest('a').as_str(),
            Some(REPO),
            Some(sha('c').as_str()),
            Utc::now(),
        )
        .await
        .unwrap();
    fx.svc.resolve_all().await.unwrap();
}

async fn labelled_artifact(fx: &Fx) {
    fx.svc
        .store
        .conn()
        .upsert_artifact(&ArtifactRow {
            digest: digest('a').as_str().to_string(),
            repository: IMAGE.to_string(),
            first_seen: Utc::now(),
            label_source: Some(REPO.to_string()),
            label_revision: Some(sha('c').as_str().to_string()),
            registry_checked_at: None,
            log_scanned_at: None,
        })
        .await
        .unwrap();
    fx.svc.resolve_all().await.unwrap();
}

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
    assert!(!open[0].dto.bulk_accept);
}

#[tokio::test]
async fn bulk_accept_needs_two_independent_rules() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut dev = unit(gitops, "api-dev");
    dev.correlation.app_label = Some("shop".into());
    let mut prod = unit(gitops, "api-prod");
    prod.correlation.app_label = Some("shop".into());
    fx.apply(gitops, vec![dev, prod]).await;

    let open = fx.open().await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].dto.rule_count, 2);
    assert!(open[0].dto.bulk_accept);
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

#[tokio::test]
async fn accepted_built_from_suggestion_becomes_an_exact_edge() {
    let fx = Fx::new().await;
    labelled_unit(&fx).await;
    let open = fx.open().await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].dto.kind, SuggestionKind::BuiltFrom);
    assert_eq!(fx.edges(LABEL_UNCONFIRMED).await.len(), 1);

    fx.svc
        .accept_suggestions(vec![open[0].dto.id.clone()], None)
        .await
        .unwrap();

    let edges = fx.edges(USER_CONFIRMED).await;
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].confidence, "exact");
    assert_eq!(edges[0].from_id, digest('a').as_str());
    assert_eq!(edges[0].to_id, commit_node_id(REPO, sha('c').as_str()));
    assert!(fx.edges(LABEL_UNCONFIRMED).await.is_empty());
    assert!(fx.open().await.is_empty());

    fx.svc.resolve_all().await.unwrap();
    assert_eq!(fx.edges(USER_CONFIRMED).await.len(), 1);
    assert!(fx.open().await.is_empty());
}

#[tokio::test]
async fn accepting_through_the_application_edit_stores_a_described_link_override() {
    let fx = Fx::new().await;
    labelled_unit(&fx).await;
    let app = fx.app("api").await;
    let suggestion = fx.open().await.remove(0).dto.id;

    fx.svc
        .edit_internal(
            app.id,
            ApplicationEdit::AcceptSuggestion {
                suggestion_id: suggestion.clone(),
            },
            Some("tester".into()),
        )
        .await
        .unwrap();

    let mut conn = fx.svc.store.conn();
    let stored = conn.list_overrides_by_application(app.id).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].kind, OverrideKind::Link.as_str());
    assert_eq!(stored[0].subject.as_deref(), Some(suggestion.as_str()));
    for part in [digest('a').as_str(), REPO, sha('c').as_str()] {
        assert!(stored[0].value_json.contains(part), "{part}");
    }
    fx.settle().await;
    let state = fx.svc.application_state(app.id).await.unwrap();
    assert_eq!(state.overrides.len(), 1);
    assert!(state.overrides[0].description.contains(REPO));
    assert_eq!(fx.edges(USER_CONFIRMED).await.len(), 1);
}

#[tokio::test]
async fn a_built_from_suggestion_without_an_application_can_be_accepted() {
    let fx = Fx::new().await;
    labelled_artifact(&fx).await;
    let open = fx.open().await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].dto.application_id, None);

    fx.svc
        .accept_suggestions(vec![open[0].dto.id.clone()], None)
        .await
        .unwrap();

    assert_eq!(fx.edges(USER_CONFIRMED).await.len(), 1);
    assert!(fx.open().await.is_empty());
}

#[tokio::test]
async fn reverting_it_restores_the_suggestion() {
    let fx = Fx::new().await;
    labelled_unit(&fx).await;
    let app = fx.app("api").await;
    let suggestion = fx.open().await.remove(0).dto.id;
    fx.svc
        .edit_internal(
            app.id,
            ApplicationEdit::AcceptSuggestion {
                suggestion_id: suggestion.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(fx.open().await.is_empty());

    let mut conn = fx.svc.store.conn();
    let stored = conn.list_overrides_by_application(app.id).await.unwrap();
    fx.svc.revert_internal(app.id, stored[0].id).await.unwrap();

    assert!(fx.edges(USER_CONFIRMED).await.is_empty());
    assert_eq!(fx.edges(LABEL_UNCONFIRMED).await.len(), 1);
    let open = fx.open().await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].dto.id, suggestion);
}

#[tokio::test]
async fn rejected_built_from_suggestion_never_returns() {
    let fx = Fx::new().await;
    let (gitops, api) = labelled_unit(&fx).await;
    let app = fx.app("api").await;
    let suggestion = fx.open().await.remove(0).dto.id;
    fx.svc
        .edit_internal(
            app.id,
            ApplicationEdit::RejectSuggestion {
                suggestion_id: suggestion.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(fx.open().await.is_empty());
    assert!(fx.edges(LABEL_UNCONFIRMED).await.is_empty());
    let mut conn = fx.svc.store.conn();
    let stored = conn.list_overrides_by_application(app.id).await.unwrap();
    assert_eq!(stored[0].kind, OverrideKind::Unlink.as_str());
    assert_eq!(stored[0].subject.as_deref(), Some(suggestion.as_str()));

    fx.svc.resolve_all().await.unwrap();
    fx.apply(gitops, vec![api]).await;
    mark_label(&fx).await;
    assert!(fx.open().await.is_empty());
    assert!(fx.edges(LABEL_UNCONFIRMED).await.is_empty());
    assert!(fx.edges(USER_CONFIRMED).await.is_empty());
}

#[tokio::test]
async fn an_ownerless_confirmation_is_listed_on_the_consumer_and_can_be_reverted() {
    let fx = Fx::new().await;
    labelled_artifact(&fx).await;
    let open = fx.open().await;
    fx.svc
        .accept_suggestions(vec![open[0].dto.id.clone()], None)
        .await
        .unwrap();
    let stored = fx.svc.store.conn().list_overrides().await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].application_id, None);

    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.targets = vec![target("1", "shop", "shop")];
    api.correlation.consumed = vec![consumed(IMAGE, digest('a'), Some(0))];
    fx.apply(gitops, vec![api]).await;
    fx.settle().await;
    let app = fx.app("api").await;

    let state = fx.svc.application_state(app.id).await.unwrap();
    assert_eq!(state.overrides.len(), 1);
    assert_eq!(state.overrides[0].id, stored[0].id);
    assert_eq!(fx.edges(USER_CONFIRMED).await.len(), 1);

    fx.svc.revert_internal(app.id, stored[0].id).await.unwrap();

    assert!(fx
        .svc
        .store
        .conn()
        .list_overrides()
        .await
        .unwrap()
        .is_empty());
    assert!(fx.edges(USER_CONFIRMED).await.is_empty());
    let reopened = fx.open().await;
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened[0].dto.id, open[0].dto.id);
}
