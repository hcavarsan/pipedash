use crate::application::services::correlation_service::tests::{
    app_ref,
    builder,
    source,
    unit,
    Fx,
};
use crate::domain::application::overrides::ApplicationEdit;
use crate::infrastructure::database::ChangeRow;

#[tokio::test]
async fn two_services_in_one_repo_stay_two_applications() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let ci = fx.provider("ci").await;
    let mut api = unit(gitops, "api");
    api.correlation.sources = vec![source("git.test/shop/mono", &["services/api"], None)];
    let mut web = unit(gitops, "web");
    web.correlation.sources = vec![source("git.test/shop/mono", &["services/web"], None)];
    fx.apply(gitops, vec![api.clone(), web.clone()]).await;
    let build = builder(
        ci,
        "api-build",
        vec![source("git.test/shop/mono", &["services/api"], None)],
    );
    fx.apply(ci, vec![build.clone()]).await;
    fx.svc.resolve_all().await.unwrap();

    assert_eq!(fx.apps().await.len(), 2);
    let mut api_members = fx.member_ids("api").await;
    api_members.sort();
    let mut expected = vec![api.id, build.id];
    expected.sort();
    assert_eq!(api_members, expected);
    assert_eq!(fx.member_ids("web").await, [web.id]);
}

#[tokio::test]
async fn same_app_ref_shares_application() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut qa = unit(gitops, "checkout-qa");
    qa.correlation.app_ref = Some(app_ref("checkout"));
    let mut prod = unit(gitops, "checkout-prod");
    prod.correlation.app_ref = Some(app_ref("checkout"));
    fx.apply(gitops, vec![qa, prod]).await;

    let apps = fx.apps().await;
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "checkout");
    assert_eq!(apps[0].identity_kind, "app_ref");
    assert_eq!(fx.members(apps[0].id).await.len(), 2);
}

#[tokio::test]
async fn build_only_pipeline_creates_no_application() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    fx.apply(
        ci,
        vec![builder(
            ci,
            "build",
            vec![source("git.test/shop/api", &[], None)],
        )],
    )
    .await;

    assert!(fx.apps().await.is_empty());
}

#[tokio::test]
async fn merge_survives_recompute_and_provider_removal() {
    let fx = Fx::new().await;
    let first = fx.provider("first").await;
    let second = fx.provider("second").await;
    let api = unit(first, "api");
    let web = unit(second, "web");
    fx.apply(first, vec![api.clone()]).await;
    fx.apply(second, vec![web.clone()]).await;
    let survivor = fx.app("api").await;
    let other = fx.app("web").await;

    fx.svc
        .edit_internal(
            survivor.id,
            ApplicationEdit::Merge {
                other_application_id: other.id,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(fx.apps().await.len(), 1);
    assert_eq!(fx.members(survivor.id).await.len(), 2);
    fx.svc.resolve_all().await.unwrap();
    assert_eq!(fx.members(survivor.id).await.len(), 2);

    fx.h.provider_service.remove_provider(first).await.unwrap();
    fx.svc.resolve_all().await.unwrap();
    let apps = fx.apps().await;
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].id, survivor.id);
    assert_eq!(apps[0].name, "api");
    let members: Vec<String> = fx
        .members(survivor.id)
        .await
        .into_iter()
        .map(|m| m.pipeline_id)
        .collect();
    assert_eq!(members, [web.id]);
}

#[tokio::test]
async fn vanished_unit_archives_application() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    fx.apply(gitops, vec![unit(gitops, "api")]).await;
    let app = fx.app("api").await;
    fx.settle().await;
    let mut conn = fx.svc.store.conn();
    conn.upsert_change(&ChangeRow {
        id: 0,
        application_id: app.id,
        repo: "git.test/shop/api".into(),
        commit_sha: "1".repeat(40),
        authored_at: None,
    })
    .await
    .unwrap();

    fx.apply(gitops, Vec::new()).await;

    assert!(fx.apps().await.is_empty());
    let stored = conn.get_application(app.id).await.unwrap().unwrap();
    assert!(stored.archived_at.is_some());
    assert_eq!(
        conn.list_changes_by_application(app.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn hidden_survives_recompute() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let api = unit(gitops, "api");
    fx.apply(gitops, vec![api.clone()]).await;
    let app = fx.app("api").await;

    fx.svc
        .edit_internal(app.id, ApplicationEdit::Hide { hidden: true }, None)
        .await
        .unwrap();
    fx.svc.resolve_all().await.unwrap();
    let mut renamed = api.clone();
    renamed.name = "api-renamed".into();
    fx.apply(gitops, vec![renamed]).await;

    assert!(fx.app("api-renamed").await.hidden);
}
