use pipedash_plugin_api::UnitId;

use crate::application::services::correlation_service::tests::{
    app_ref,
    target,
    unit,
    Fx,
};
use crate::domain::application::overrides::ApplicationEdit;

#[tokio::test]
async fn shared_namespace_keeps_separate_target_names() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut api = unit(gitops, "api");
    api.correlation.targets = vec![target("1", "shop", "Production")];
    let mut web = unit(gitops, "web");
    web.correlation.targets = vec![target("1", "shop", "Prod")];
    fx.apply(gitops, vec![api, web]).await;
    let api_app = fx.app("api").await;
    let web_app = fx.app("web").await;

    fx.svc
        .edit_internal(
            api_app.id,
            ApplicationEdit::TargetName {
                group_key: "uid:1|shop".into(),
                name: "Live".into(),
            },
            None,
        )
        .await
        .unwrap();

    let mut conn = fx.svc.store.conn();
    let api_rows = conn
        .list_application_targets_by_application(api_app.id)
        .await
        .unwrap();
    let web_rows = conn
        .list_application_targets_by_application(web_app.id)
        .await
        .unwrap();
    assert_eq!(api_rows[0].name, "Production");
    assert_eq!(web_rows[0].name, "Prod");
    fx.settle().await;
    let api_state = fx.svc.application_state(api_app.id).await.unwrap();
    let web_state = fx.svc.application_state(web_app.id).await.unwrap();
    assert_eq!(api_state.groups[0].label, "Live");
    assert_eq!(web_state.groups[0].label, "Prod");
}

#[tokio::test]
async fn first_seen_order_creates_no_direction() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut qa = unit(gitops, "checkout-qa");
    qa.correlation.app_ref = Some(app_ref("checkout"));
    qa.correlation.targets = vec![target("1", "qa", "qa")];
    fx.apply(gitops, vec![qa.clone()]).await;
    let mut prod = unit(gitops, "checkout-prod");
    prod.correlation.app_ref = Some(app_ref("checkout"));
    prod.correlation.targets = vec![target("2", "prod", "prod")];
    fx.apply(gitops, vec![qa, prod]).await;

    let app = fx.app("checkout").await;
    let mut conn = fx.svc.store.conn();
    assert_eq!(
        conn.list_application_targets_by_application(app.id)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(conn
        .list_target_directions_by_application(app.id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn unit_parent_groups_targets() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut first = unit(gitops, "shop-eu");
    first.correlation.app_ref = Some(app_ref("shop"));
    first.correlation.unit_parent = UnitId::new("shop-set");
    first.correlation.targets = vec![target("1", "eu", "eu")];
    let mut second = unit(gitops, "shop-us");
    second.correlation.app_ref = Some(app_ref("shop"));
    second.correlation.unit_parent = UnitId::new("shop-set");
    second.correlation.targets = vec![target("2", "us", "us")];
    fx.apply(gitops, vec![first, second]).await;

    let app = fx.app("shop").await;
    let mut conn = fx.svc.store.conn();
    let rows = conn
        .list_application_targets_by_application(app.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].group_key, rows[1].group_key);
    assert!(rows[0].group_key.starts_with("parent:"));
}

#[tokio::test]
async fn provider_direction_comes_only_from_declared_dependency() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let mut qa = unit(gitops, "shop-qa");
    qa.correlation.app_ref = Some(app_ref("shop"));
    qa.correlation.targets = vec![target("1", "qa", "qa")];
    let mut prod = unit(gitops, "shop-prod");
    prod.correlation.app_ref = Some(app_ref("shop"));
    prod.correlation.targets = vec![target("2", "prod", "prod")];
    prod.correlation.unit_depends_on = vec![UnitId::new("shop-qa").unwrap()];
    fx.apply(gitops, vec![qa, prod]).await;

    let app = fx.app("shop").await;
    let mut conn = fx.svc.store.conn();
    let directions = conn
        .list_target_directions_by_application(app.id)
        .await
        .unwrap();
    assert_eq!(directions.len(), 1);
    assert_eq!(directions[0].from_group, "uid:1|qa");
    assert_eq!(directions[0].to_group, "uid:2|prod");
    assert_eq!(directions[0].source, "provider");
}
