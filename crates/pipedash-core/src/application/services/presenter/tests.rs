use chrono::Utc;
use pipedash_plugin_api::{
    ActionAvailability,
    PipelineActionState,
    RunActionState,
    RunDisplay,
    AUTH_MODE_KEY,
};
use serde_json::json;

use super::READ_ONLY_PROVIDER;
use crate::application::test_support::plugins::STUB_FORCE;
use crate::application::test_support::{
    stub_run,
    Harness,
    StubProvider,
};
use crate::domain::{
    Pipeline,
    PipelineRun,
    PipelineStatus,
};

fn running_pipeline(provider_id: i64, provider_type: &str) -> Pipeline {
    serde_json::from_value(json!({
        "id": format!("{provider_type}__{provider_id}__build"), "provider_id": provider_id,
        "provider_type": provider_type, "name": "build", "status": "running",
        "repository": "build", "last_updated": Utc::now(),
        "metadata": { "can_enable": true }
    }))
    .unwrap()
}

fn running_run(pipeline: &Pipeline) -> PipelineRun {
    let mut run = stub_run(&pipeline.id, 1, 0);
    run.status = PipelineStatus::Running;
    run
}

#[tokio::test]
async fn presenter_should_disallow_mutations_for_no_auth_provider() {
    let h = Harness::new().await;
    let (public_id, _) = h.add_stub(StubProvider::new).await;
    let (signed_id, _) = h.add_stub(StubProvider::new).await;
    let mut config = h.repository.get_provider(public_id).await.unwrap();
    config
        .config
        .insert(AUTH_MODE_KEY.to_string(), "none".to_string());
    h.repository
        .update_provider(public_id, &config)
        .await
        .unwrap();
    let mut pipelines = [
        running_pipeline(public_id, STUB_FORCE),
        running_pipeline(signed_id, STUB_FORCE),
    ];

    h.presenter.pipelines(&mut pipelines).await.unwrap();

    let blocked = ActionAvailability::blocked(READ_ONLY_PROVIDER);
    let [public, signed] = &pipelines;
    assert_eq!(public.actions.trigger, blocked);
    assert_eq!(public.actions.cancel, blocked);
    assert_eq!(public.actions.force_cancel, blocked);
    assert_eq!(public.actions.enable, blocked);
    assert!(signed.actions.trigger.allowed);
    assert!(signed.actions.cancel.allowed);
    assert!(signed.actions.force_cancel.allowed);
    assert!(signed.actions.enable.allowed);

    let mut public_runs = [running_run(public)];
    let mut signed_runs = [running_run(signed)];
    h.presenter.runs(public, &mut public_runs).await.unwrap();
    h.presenter.runs(signed, &mut signed_runs).await.unwrap();

    let actions = &public_runs[0].actions;
    assert_eq!(actions.cancel, blocked);
    assert_eq!(actions.force_cancel, blocked);
    assert_eq!(actions.rerun, blocked);
    assert_eq!(actions.rerun_failed, blocked);
    assert_eq!(actions.review, blocked);
    assert!(signed_runs[0].actions.cancel.allowed);
    assert!(signed_runs[0].actions.force_cancel.allowed);
    assert!(public_runs[0].pace.live);
    assert_eq!(public_runs[0].display.title, "#1");
}

#[tokio::test]
async fn presenter_should_leave_default_state_for_unregistered_provider() {
    let h = Harness::new().await;
    let (provider_id, _) = h.add_stub(StubProvider::new).await;
    let mut pipelines = [running_pipeline(provider_id, "unregistered")];

    h.presenter.pipelines(&mut pipelines).await.unwrap();
    assert_eq!(pipelines[0].actions, PipelineActionState::default());

    let mut runs = [running_run(&pipelines[0])];
    h.presenter.runs(&pipelines[0], &mut runs).await.unwrap();
    assert_eq!(runs[0].actions, RunActionState::default());
    assert_eq!(runs[0].display, RunDisplay::default());
}
