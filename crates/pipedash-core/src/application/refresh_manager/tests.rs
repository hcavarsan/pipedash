use super::*;
use crate::application::test_support::{
    Gate,
    Harness,
    StubProvider,
};
use crate::domain::RefreshState;
use crate::event::CoreEvent;

#[tokio::test]
async fn the_first_tick_fetches_each_due_provider_once() {
    let h = Harness::new().await;
    let (_, first) = h.add_stub(StubProvider::new).await;
    let (_, second) = h.add_stub(StubProvider::new).await;
    h.provider_service.load_all_providers().await.unwrap();

    h.start().await;
    h.start().await;
    h.bus
        .wait_for("both providers", |events| {
            events
                .iter()
                .filter(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
                .count()
                == 2
        })
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!((first.fetch_count(), second.fetch_count()), (1, 1));
    assert_eq!(h.bus.count("pipelines-updated"), 2);
}

#[tokio::test]
async fn providers_missing_from_the_startup_load_are_fetched_on_schedule() {
    let h = Harness::new().await;
    let (_, stub) = h.add_stub(StubProvider::new).await;

    h.start().await;
    h.bus
        .wait_for("the unloaded provider", |events| {
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
        })
        .await;

    assert_eq!(stub.fetch_count(), 1);
}

#[tokio::test]
async fn idle_mode_keeps_scheduled_fetches_from_starting() {
    let h = Harness::new().await;
    let (_, stub) = h.add_stub(StubProvider::new).await;
    h.provider_service.load_all_providers().await.unwrap();
    h.refresh_manager.set_mode(RefreshMode::Idle);

    h.start().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(stub.fetch_count(), 0);
    h.refresh_manager.set_mode(RefreshMode::Active);
}

#[tokio::test]
async fn stopping_cancels_running_attempts_and_their_waiters() {
    let h = Harness::new().await;
    h.start().await;
    let gate = Gate::new();
    let (id, stub) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
    let waiting = tokio::spawn({
        let service = Arc::clone(&h.pipeline_service);
        async move { service.fetch_now(&[id], Duration::from_secs(10)).await }
    });
    gate.arrived().await;

    h.refresh_manager.stop().await;
    let results = waiting.await.unwrap();
    h.refresh_manager.start().await;

    assert_eq!(results[0].state, RefreshState::Pending);
    let view = h.scheduler.view(id, Instant::now()).unwrap();
    assert_eq!((view.failures, view.in_flight), (0, false));
    assert_eq!(stub.fetch_count(), 1);
    assert_eq!(h.bus.count("pipelines-updated"), 0);
    assert!(h.refresh_manager.task.lock().unwrap().is_none());
}

#[tokio::test]
async fn a_stopped_manager_hands_new_claims_back_as_cancelled() {
    let h = Harness::new().await;
    h.start().await;
    h.refresh_manager.stop().await;
    let (id, _) = h.add_stub(StubProvider::new).await;

    let results = h
        .pipeline_service
        .fetch_now(&[id], Duration::from_millis(300))
        .await;

    assert_eq!(results[0].state, RefreshState::Pending);
}
