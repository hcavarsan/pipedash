use crate::application::services::correlation_service::tests::{
    builder,
    source,
    target,
    unit,
    Fx,
};
use crate::domain::application::board::{
    ApplicationBoard,
    ApplicationBoardQuery,
};
use crate::domain::application::overrides::ApplicationEdit;
use crate::domain::application::Stage;
use crate::domain::Pipeline;

async fn board(fx: &Fx, search: Option<&str>) -> ApplicationBoard {
    fx.settle().await;
    fx.svc
        .board(ApplicationBoardQuery {
            search: search.map(str::to_string),
        })
        .await
        .unwrap()
}

async fn discovered(fx: &Fx) -> (Pipeline, Pipeline) {
    let ci = fx.provider("ci").await;
    let gitops = fx.provider("gitops").await;
    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let mut api = unit(gitops, "api");
    api.correlation.targets = vec![target("1", "prod", "prod"), target("1", "stage", "stage")];
    fx.apply(gitops, vec![api.clone()]).await;
    (build, api)
}

#[tokio::test]
async fn discovered_units_are_applications_and_the_rest_is_unassigned() {
    let fx = Fx::new().await;
    let (build, api) = discovered(&fx).await;

    let board = board(&fx, None).await;

    let unassigned: Vec<&str> = board
        .unassigned
        .iter()
        .map(|card| card.pipeline_id.as_str())
        .collect();
    assert_eq!(unassigned, [build.id.as_str()]);
    assert_eq!(board.applications.len(), 1);
    let app = &board.applications[0];
    assert_eq!(app.name, "api");
    assert!(app.editable);
    assert_eq!(app.notice, None);
    assert!(app.build.is_empty());
    assert_eq!(app.deploy[0].pipeline_id, api.id);
    let labels: Vec<&str> = app
        .environments
        .iter()
        .map(|environment| environment.label.as_str())
        .collect();
    assert_eq!(labels, ["prod", "stage"]);
}

#[tokio::test]
async fn linking_a_pipeline_moves_it_from_unassigned_to_the_lane() {
    let fx = Fx::new().await;
    let (build, _) = discovered(&fx).await;
    let id = fx.app("api").await.id;
    let link = ApplicationEdit::Link {
        pipeline_id: build.id.clone(),
        stage: Some(Stage::Build),
    };
    fx.svc.apply_edit(id, link, None).await.unwrap();

    let linked = board(&fx, None).await;
    assert!(linked.unassigned.is_empty());
    assert_eq!(linked.applications[0].build[0].pipeline_id, build.id);

    let unlink = ApplicationEdit::Unlink {
        pipeline_id: build.id.clone(),
    };
    fx.svc.apply_edit(id, unlink, None).await.unwrap();

    let unlinked = board(&fx, None).await;
    assert_eq!(unlinked.unassigned[0].pipeline_id, build.id);
    assert!(unlinked.applications[0].build.is_empty());
}

#[tokio::test]
async fn grouping_a_target_with_an_environment_joins_them_under_its_label() {
    let fx = Fx::new().await;
    discovered(&fx).await;
    let id = fx.app("api").await.id;
    let before = board(&fx, None).await;
    let environments = &before.applications[0].environments;
    let group = ApplicationEdit::TargetGroup {
        target_keys: vec![
            environments[1].targets[0].target_key.clone(),
            environments[0].targets[0].target_key.clone(),
        ],
        group_name: Some(environments[0].label.clone()),
    };
    fx.svc.apply_edit(id, group, None).await.unwrap();

    let after = board(&fx, None).await;
    let environments = &after.applications[0].environments;
    assert_eq!(environments.len(), 1);
    assert_eq!(environments[0].label, "prod");
    let mut names: Vec<&str> = environments[0]
        .targets
        .iter()
        .map(|target| target.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["prod", "stage"]);
}

#[tokio::test]
async fn search_keeps_matching_pipelines_and_applications() {
    let fx = Fx::new().await;
    discovered(&fx).await;

    let by_pipeline = board(&fx, Some(" BUILD ")).await;
    assert_eq!(by_pipeline.unassigned.len(), 1);
    assert!(by_pipeline.applications.is_empty());

    let by_application = board(&fx, Some("api")).await;
    assert!(by_application.unassigned.is_empty());
    assert_eq!(by_application.applications.len(), 1);
}
