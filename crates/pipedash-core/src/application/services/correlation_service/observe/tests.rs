use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use crate::application::services::correlation_service::tests::{
    artifact,
    builder,
    consumed,
    digest,
    run_with,
    source,
    target,
    unit,
    Fx,
};
use crate::application::test_support::{
    Gate,
    StubProvider,
};
use crate::domain::{
    Pipeline,
    PipelineStatus,
    ResourceState,
};

const IMAGE: &str = "registry.test/shop/api";

struct Joined {
    fx: Fx,
    ci: i64,
    stub: Arc<StubProvider>,
    build: Pipeline,
}

async fn joined(gate: Option<&Arc<Gate>>) -> Joined {
    if let Some(gate) = gate {
        gate.open();
    }
    let fx = Fx::new().await;
    fx.h.pipeline_service.set_observer(fx.svc.clone());
    let (ci, stub) =
        fx.h.add_stub(|id| match gate {
            Some(gate) => StubProvider::new(id).run_gated(gate),
            None => StubProvider::new(id),
        })
        .await;
    let gitops = fx.provider("gitops").await;
    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let run = run_with(
        &build.id,
        1,
        vec![source("git.test/shop/api", &[], Some('1'))],
        vec![artifact(IMAGE, Some(digest('a')), None)],
    );
    fx.runs(ci, &build.id, vec![run]).await;
    let mut api = unit(gitops, "api-prod");
    api.correlation.sources = vec![source("git.test/shop/config", &["apps/api"], Some('2'))];
    api.correlation.targets = vec![target("1", "shop", "shop")];
    api.correlation.consumed = vec![consumed(IMAGE, digest('a'), Some(0))];
    fx.apply(gitops, vec![api]).await;
    fx.settle().await;
    assert!(fx.member_ids("api-prod").await.contains(&build.id));
    if let Some(gate) = gate {
        gate.close();
    }
    Joined {
        fx,
        ci,
        stub,
        build,
    }
}

fn advanced(pipeline: &Pipeline) -> Pipeline {
    let mut next = pipeline.clone();
    next.last_run = Some(Utc::now());
    next
}

fn with_status(pipeline: &Pipeline, status: PipelineStatus) -> Pipeline {
    let mut next = pipeline.clone();
    next.status = status;
    next
}

async fn until(done: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("condition never held");
}

async fn emitted(fx: &Fx) -> usize {
    fx.settle().await;
    fx.svc.flush_events().await;
    fx.h.bus.count("applications-changed")
}

#[tokio::test]
async fn a_member_whose_last_run_advances_gets_its_history_fetched() {
    let j = joined(None).await;
    let before = j.stub.history_fetch_count();
    *j.stub.runs.lock().unwrap() = vec![run_with(
        &j.build.id,
        2,
        vec![source("git.test/shop/api", &[], Some('2'))],
        vec![artifact(IMAGE, Some(digest('b')), None)],
    )];

    j.fx.apply(j.ci, vec![advanced(&j.build)]).await;
    j.fx.settle().await;

    assert_eq!(j.stub.history_fetch_count(), before + 1);
    let mut conn = j.fx.svc.store.conn();
    let numbers: Vec<i64> = conn
        .list_run_evidence_by_pipeline(&j.build.id)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.run_number)
        .collect();
    assert!(numbers.contains(&2));
}

#[tokio::test]
async fn a_member_whose_status_changes_gets_its_history_fetched() {
    let j = joined(None).await;
    let before = j.stub.history_fetch_count();

    let failed = with_status(&j.build, PipelineStatus::Failed);
    j.fx.apply(j.ci, vec![failed]).await;
    j.fx.settle().await;

    assert_eq!(j.stub.history_fetch_count(), before + 1);
}

#[tokio::test]
async fn an_unchanged_refresh_fetches_no_history() {
    let j = joined(None).await;
    let before = j.stub.history_fetch_count();

    let mut again = j.build.clone();
    again.last_updated = Utc::now();
    j.fx.apply(j.ci, vec![again]).await;
    j.fx.settle().await;

    assert_eq!(j.stub.history_fetch_count(), before);
}

#[tokio::test]
async fn a_pipeline_that_is_not_a_member_never_fetches_history() {
    let fx = Fx::new().await;
    fx.h.pipeline_service.set_observer(fx.svc.clone());
    let (ci, stub) = fx.h.add_stub(StubProvider::new).await;
    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;

    let next = with_status(&advanced(&build), PipelineStatus::Failed);
    fx.apply(ci, vec![next]).await;
    fx.settle().await;

    assert!(fx.apps().await.is_empty());
    assert_eq!(stub.history_fetch_count(), 0);
}

#[tokio::test]
async fn refreshes_during_a_running_fetch_share_one_more_fetch() {
    let gate = Gate::new();
    let j = joined(Some(&gate)).await;
    let before = j.stub.history_fetch_count();

    j.fx.apply(j.ci, vec![advanced(&j.build)]).await;
    until(|| j.stub.history_fetch_count() == before + 1).await;
    for status in [PipelineStatus::Failed, PipelineStatus::Running] {
        j.fx.apply(j.ci, vec![with_status(&j.build, status)]).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(j.stub.history_fetch_count(), before + 1);

    gate.open();
    j.fx.settle().await;
    assert_eq!(j.stub.history_fetch_count(), before + 2);
}

#[tokio::test]
async fn a_status_only_change_of_a_unit_emits_one_applications_changed() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let api = unit(gitops, "api");
    fx.apply(gitops, vec![api.clone()]).await;
    let before = emitted(&fx).await;

    fx.apply(gitops, vec![with_status(&api, PipelineStatus::Failed)])
        .await;

    assert_eq!(emitted(&fx).await, before + 1);
}

#[tokio::test]
async fn a_health_label_change_of_a_unit_emits_one_applications_changed() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let api = unit(gitops, "api");
    fx.apply(gitops, vec![api.clone()]).await;
    let before = emitted(&fx).await;

    let mut degraded = api.clone();
    degraded.resource_state = Some(ResourceState::new("Degraded", None));
    fx.apply(gitops, vec![degraded]).await;

    assert_eq!(emitted(&fx).await, before + 1);
}

#[tokio::test]
async fn an_unchanged_refresh_emits_no_applications_changed() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let api = unit(gitops, "api");
    fx.apply(gitops, vec![api.clone()]).await;
    let before = emitted(&fx).await;

    let mut again = api.clone();
    again.last_updated = Utc::now();
    fx.apply(gitops, vec![again]).await;

    assert_eq!(emitted(&fx).await, before);
}

#[tokio::test]
async fn a_status_change_of_a_non_member_emits_no_applications_changed() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let before = emitted(&fx).await;

    fx.apply(ci, vec![with_status(&build, PipelineStatus::Failed)])
        .await;

    assert!(fx.apps().await.is_empty());
    assert_eq!(emitted(&fx).await, before);
}
