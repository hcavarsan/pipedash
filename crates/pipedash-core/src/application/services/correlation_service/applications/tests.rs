use crate::application::services::correlation_service::tests::{
    app_ref,
    builder,
    run_with,
    source,
    target,
    unit,
    Fx,
};
use crate::domain::application::overrides::ApplicationEdit;
use crate::domain::DomainError;
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
async fn removing_the_last_provider_archives_its_applications() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.targets = vec![target("1", "shop", "prod")];
    fx.apply(gitops, vec![api]).await;
    let app = fx.app("api").await;
    assert!(!fx.members(app.id).await.is_empty());
    assert!(!fx.svc.detail(app.id).await.unwrap().members.is_empty());
    fx.svc.flush_events().await;
    let before = fx.h.bus.count("applications-changed");

    fx.h.provider_service.remove_provider(gitops).await.unwrap();

    assert!(fx.apps().await.is_empty());
    assert!(fx
        .svc
        .list(Default::default())
        .await
        .unwrap()
        .applications
        .is_empty());
    assert!(matches!(
        fx.svc.detail(app.id).await,
        Err(DomainError::NotFound(_))
    ));
    assert_eq!(fx.h.bus.count("applications-changed"), before + 1);
}

#[tokio::test]
async fn removing_one_provider_keeps_the_others_applications() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let other = fx.provider("other").await;
    let ci = fx.provider("ci").await;
    let mut api = unit(gitops, "api");
    api.correlation.sources = vec![source("git.test/shop/api", &[], None)];
    let web = unit(other, "web");
    let build = builder(
        ci,
        "api-build",
        vec![source("git.test/shop/api", &[], None)],
    );
    fx.apply(gitops, vec![api.clone()]).await;
    fx.apply(other, vec![web.clone()]).await;
    fx.apply(ci, vec![build.clone()]).await;
    let build_run = run_with(
        &build.id,
        1,
        vec![source("git.test/shop/api", &[], None)],
        vec![],
    );
    fx.runs(ci, &build.id, vec![build_run]).await;
    let api_run = run_with(
        &api.id,
        1,
        vec![source("git.test/shop/api", &[], None)],
        vec![],
    );
    fx.runs(gitops, &api.id, vec![api_run]).await;
    let mut before = fx.member_ids("api").await;
    before.sort();
    let mut expected = vec![api.id.clone(), build.id.clone()];
    expected.sort();
    assert_eq!(before, expected);

    let evidence = fx.svc.store.conn().list_run_evidence().await.unwrap();
    assert_eq!(evidence.len(), 2);

    fx.h.provider_service.remove_provider(ci).await.unwrap();

    let left = fx.svc.store.conn().list_run_evidence().await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].pipeline_id, api.id);
    assert_eq!(fx.apps().await.len(), 2);
    assert_eq!(fx.member_ids("api").await, [api.id]);
    assert_eq!(fx.member_ids("web").await, [web.id]);

    fx.h.provider_service.remove_provider(other).await.unwrap();

    let names: Vec<String> = fx.apps().await.into_iter().map(|a| a.name).collect();
    assert_eq!(names, ["api"]);
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
