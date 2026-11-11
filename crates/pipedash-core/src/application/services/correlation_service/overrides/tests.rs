use crate::application::services::correlation_service::tests::{
    builder,
    source,
    unit,
    Fx,
};
use crate::domain::application::overrides::{
    ApplicationEdit,
    CreateApplicationInput,
    CreateApplicationMember,
};
use crate::domain::DomainError;

#[tokio::test]
async fn only_user_applications_can_be_deleted() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let ci = fx.provider("ci").await;
    fx.apply(gitops, vec![unit(gitops, "api")]).await;
    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let automatic = fx.app("api").await;

    let denied = fx.svc.delete_application(automatic.id).await;
    assert!(matches!(denied, Err(DomainError::InvalidConfig(_))));

    let created = fx
        .svc
        .create_internal(
            CreateApplicationInput {
                name: "Shared tooling".into(),
                members: vec![CreateApplicationMember {
                    pipeline_id: build.id.clone(),
                    stage: None,
                }],
            },
            Some("tester".into()),
        )
        .await
        .unwrap();
    assert_eq!(fx.member_ids("Shared tooling").await, [build.id]);

    fx.svc.delete_application(created).await.unwrap();
    assert!(fx.apps().await.iter().all(|a| a.id != created));
}

#[tokio::test]
async fn rename_is_an_override_that_can_be_reverted() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    fx.apply(gitops, vec![unit(gitops, "api")]).await;
    let app = fx.app("api").await;

    fx.svc
        .edit_internal(
            app.id,
            ApplicationEdit::Rename {
                name: "Storefront".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(fx.app("Storefront").await.id, app.id);

    let mut conn = fx.svc.store.conn();
    let stored = conn.list_overrides_by_application(app.id).await.unwrap();
    fx.svc.revert_internal(app.id, stored[0].id).await.unwrap();
    assert_eq!(fx.app("api").await.id, app.id);
}

#[tokio::test]
async fn accepting_overlapping_merge_suggestions_stores_no_redundant_override() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let units: Vec<_> = ["api", "web", "worker"]
        .iter()
        .map(|name| {
            let mut unit = unit(gitops, name);
            unit.correlation.app_label = Some("shop".into());
            unit
        })
        .collect();
    fx.apply(gitops, units).await;
    let ids: Vec<String> = fx.open().await.into_iter().map(|s| s.dto.id).collect();
    assert_eq!(ids.len(), 3);

    fx.svc.accept_suggestions(ids.clone(), None).await.unwrap();

    assert_eq!(fx.apps().await.len(), 1);
    let mut conn = fx.svc.store.conn();
    assert_eq!(conn.list_overrides().await.unwrap().len(), 2);
    assert!(fx.open().await.is_empty());

    fx.svc.accept_suggestions(ids, None).await.unwrap();
    let mut conn = fx.svc.store.conn();
    assert_eq!(conn.list_overrides().await.unwrap().len(), 2);
}
