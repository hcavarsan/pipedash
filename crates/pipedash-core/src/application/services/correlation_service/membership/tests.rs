use crate::application::services::correlation_service::tests::{
    artifact,
    builder,
    consumed,
    digest,
    run_with,
    source,
    unit,
    Fx,
};
use crate::domain::application::overrides::ApplicationEdit;

#[tokio::test]
async fn unit_without_source_has_no_build_member() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let ci = fx.provider("ci").await;
    let api = unit(gitops, "api");
    fx.apply(gitops, vec![api.clone()]).await;
    fx.apply(
        ci,
        vec![builder(
            ci,
            "build",
            vec![source("git.test/shop/api", &[], None)],
        )],
    )
    .await;

    assert_eq!(fx.member_ids("api").await, [api.id]);
}

#[tokio::test]
async fn sources_do_not_cross_join_paths() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let ci = fx.provider("ci").await;
    let mut api = unit(gitops, "api");
    api.correlation.sources = vec![
        source("git.test/a/repo", &["x"], None),
        source("git.test/b/repo", &["y"], None),
    ];
    fx.apply(gitops, vec![api.clone()]).await;
    fx.apply(
        ci,
        vec![builder(
            ci,
            "build",
            vec![source("git.test/a/repo", &["y"], None)],
        )],
    )
    .await;

    assert_eq!(fx.member_ids("api").await, [api.id]);
}

#[tokio::test]
async fn revert_unlink_restores_link() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let ci = fx.provider("ci").await;
    let image = "registry.test/shop/api";
    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let run = run_with(
        &build.id,
        7,
        Vec::new(),
        vec![artifact(image, Some(digest('a')), None)],
    );
    fx.runs(ci, &build.id, vec![run]).await;
    let mut api = unit(gitops, "api");
    api.correlation.consumed = vec![consumed(image, digest('a'), None)];
    fx.apply(gitops, vec![api.clone()]).await;
    let app = fx.app("api").await;
    assert!(fx.member_ids("api").await.contains(&build.id));

    fx.svc
        .edit_internal(
            app.id,
            ApplicationEdit::Unlink {
                pipeline_id: build.id.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(fx.member_ids("api").await, [api.id.clone()]);

    let mut conn = fx.svc.store.conn();
    let overrides = conn.list_overrides_by_application(app.id).await.unwrap();
    assert_eq!(overrides.len(), 1);
    fx.svc
        .revert_internal(app.id, overrides[0].id)
        .await
        .unwrap();
    assert!(fx.member_ids("api").await.contains(&build.id));
}
