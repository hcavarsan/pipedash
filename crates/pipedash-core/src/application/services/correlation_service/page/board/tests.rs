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
    BoardCard,
    BoardDrop,
    BoardDropOutcome,
    BoardZone,
};
use crate::domain::application::overrides::ApplicationEdit;
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

async fn drop_on(fx: &Fx, card: &BoardCard, zone: &BoardZone) -> BoardDropOutcome {
    let drop = BoardDrop {
        card: card.item.clone(),
        zone: zone.target.clone(),
    };
    fx.svc.board_drop(drop, None).await.unwrap()
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

    assert_eq!(board.unassigned.pipelines.len(), 1);
    let free = &board.unassigned.pipelines[0].card;
    assert_eq!(free.item.key, build.id);
    assert_eq!(free.zone_id, board.unassigned.zone.id);
    assert_eq!(board.applications.len(), 1);
    let app = &board.applications[0];
    assert_eq!(app.name, "api");
    assert_eq!(app.notice, None);
    assert!(app.build.pipelines.is_empty());
    assert!(app.build.zone.enabled);
    assert_eq!(app.build.zone.drop_class, free.drop_class);
    assert_eq!(app.deploy.pipelines[0].card.item.key, api.id);
    let labels: Vec<&str> = app
        .environments
        .iter()
        .map(|environment| environment.label.as_str())
        .collect();
    assert_eq!(labels, ["prod", "stage"]);
    let target = &app.environments[0].targets[0].card;
    assert_eq!(target.drop_class, app.environments[1].zone.drop_class);
    assert_ne!(target.drop_class, app.build.zone.drop_class);
}

#[tokio::test]
async fn dropping_a_pipeline_links_restages_and_unlinks_it() {
    let fx = Fx::new().await;
    let (build, _) = discovered(&fx).await;
    let before = board(&fx, None).await;
    let card = &before.unassigned.pipelines[0].card;
    let app = &before.applications[0];

    assert_eq!(
        drop_on(&fx, card, &app.build.zone).await,
        BoardDropOutcome::Applied
    );
    let linked = board(&fx, None).await;
    assert!(linked.unassigned.pipelines.is_empty());
    let member = &linked.applications[0].build.pipelines[0].card;
    assert_eq!(member.item.key, build.id);

    drop_on(&fx, member, &linked.applications[0].deploy.zone).await;
    let restaged = board(&fx, None).await;
    assert!(restaged.applications[0].build.pipelines.is_empty());
    let deploys = &restaged.applications[0].deploy.pipelines;
    let moved = deploys
        .iter()
        .find(|p| p.card.item.key == build.id)
        .unwrap();

    drop_on(&fx, &moved.card, &restaged.unassigned.zone).await;
    let unlinked = board(&fx, None).await;
    assert_eq!(unlinked.unassigned.pipelines[0].card.item.key, build.id);
}

#[tokio::test]
async fn dropping_a_target_on_an_environment_keeps_that_environment_and_its_arrows() {
    let fx = Fx::new().await;
    discovered(&fx).await;
    let before = board(&fx, None).await;
    let app = &before.applications[0];
    let prod = app.environments[0].zone.target.key.clone().unwrap();
    let stage = app.environments[1].zone.target.key.clone().unwrap();
    let direction = ApplicationEdit::Direction {
        from_group: stage.clone(),
        to_group: prod.clone(),
        present: true,
    };
    fx.svc.apply_edit(app.id, direction, None).await.unwrap();
    let mut extra = unit(fx.provider("gitops-2").await, "api-dev");
    extra.correlation.targets = vec![target("1", "dev", "dev")];
    fx.apply(extra.provider_id, vec![extra.clone()]).await;
    let link = ApplicationEdit::Link {
        pipeline_id: extra.id.clone(),
        stage: None,
    };
    fx.svc.apply_edit(app.id, link, None).await.unwrap();

    let grown = board(&fx, None).await;
    let environments = &grown.applications[0].environments;
    let dev = environments.iter().find(|e| e.label == "dev").unwrap();
    let prod_zone = environments
        .iter()
        .find(|e| e.zone.target.key.as_deref() == Some(prod.as_str()))
        .unwrap();
    drop_on(&fx, &dev.targets[0].card, &prod_zone.zone).await;

    let after = board(&fx, None).await;
    let environments = &after.applications[0].environments;
    assert_eq!(environments.len(), 2);
    let joined = environments
        .iter()
        .find(|e| e.zone.target.key.as_deref() == Some(prod.as_str()))
        .unwrap();
    assert_eq!(joined.label, "prod");
    assert_eq!(joined.targets.len(), 2);
    let state = fx.svc.application_state(app.id).await.unwrap();
    assert_eq!(state.directions.len(), 1);
    assert_eq!(state.directions[0].from_group, stage);
    assert_eq!(state.directions[0].to_group, prod);
}

#[tokio::test]
async fn drops_that_need_a_name_ask_for_it_and_wrong_drops_are_refused() {
    let fx = Fx::new().await;
    let (build, _) = discovered(&fx).await;
    let board = board(&fx, None).await;
    let pipeline = &board.unassigned.pipelines[0].card;
    let app = &board.applications[0];
    let target = &app.environments[0].targets[0].card;

    assert_eq!(
        drop_on(&fx, pipeline, &board.new_application).await,
        BoardDropOutcome::NameApplication {
            pipeline_id: build.id.clone()
        }
    );
    assert_eq!(
        drop_on(&fx, target, app.new_environment.as_ref().unwrap()).await,
        BoardDropOutcome::NameEnvironment {
            application_id: app.id,
            target_key: target.item.key.clone()
        }
    );
    for (card, zone) in [
        (pipeline, &app.environments[0].zone),
        (pipeline, &board.unassigned.zone),
        (target, &app.build.zone),
    ] {
        let drop = BoardDrop {
            card: card.item.clone(),
            zone: zone.target.clone(),
        };
        assert!(fx.svc.board_drop(drop, None).await.is_err());
    }
}

#[tokio::test]
async fn search_keeps_matching_pipelines_and_applications() {
    let fx = Fx::new().await;
    discovered(&fx).await;

    let by_pipeline = board(&fx, Some(" BUILD ")).await;
    assert_eq!(by_pipeline.unassigned.pipelines.len(), 1);
    assert!(by_pipeline.applications.is_empty());

    let by_application = board(&fx, Some("api")).await;
    assert!(by_application.unassigned.pipelines.is_empty());
    assert_eq!(by_application.applications.len(), 1);
}
