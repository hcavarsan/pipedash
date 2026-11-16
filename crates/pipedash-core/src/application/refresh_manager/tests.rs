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
async fn background_mode_keeps_refreshing_but_no_sooner_than_its_floor() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    h.provider_service.load_all_providers().await.unwrap();
    h.refresh_manager.set_mode(RefreshMode::Background);

    h.start().await;
    h.bus
        .wait_for("the background fetch", |events| {
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
        })
        .await;
    assert_eq!(stub.fetch_count(), 1);

    let floor = h.scheduler.policy().background_floor;
    let soon = Instant::now() + floor - Duration::from_secs(1);
    assert!(h.pipeline_service.claim_due(soon).await.is_empty());
    let due = h
        .pipeline_service
        .claim_due(soon + Duration::from_secs(2))
        .await;
    assert_eq!(due.iter().map(FetchClaim::id).collect::<Vec<_>>(), vec![id]);

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

#[tokio::test]
async fn slow_overdue_history_does_not_block_a_healthy_provider() {
    use crate::application::fetch_scheduler::ForcedClaim;
    use crate::application::services::pipeline_service::FETCH_CONCURRENCY;
    use crate::application::test_support::pipeline;
    use crate::domain::PipelineStatus;

    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, slow) = h
        .add_stub(|id| {
            StubProvider::new(id)
                .returning(vec![pipeline(id, "active", PipelineStatus::Running)])
                .run_gated(&gate)
        })
        .await;
    h.provider_service.load_all_providers().await.unwrap();
    let lease = h.provider_service.acquire(id).await.unwrap();
    let (dispatcher, commands) = RefreshDispatcher::channel();
    let cancel = CancellationToken::new();
    let manager = tokio::spawn(run(
        Arc::clone(&h.pipeline_service),
        None,
        watch::channel(RefreshMode::Idle).1,
        cancel.clone(),
        commands,
    ));
    for attempt in 1..=FETCH_CONCURRENCY {
        let ForcedClaim::Claimed(claim) = h.scheduler.claim_forced(
            id,
            lease.generation,
            Instant::now() + Duration::from_secs(attempt as u64 * 60),
        ) else {
            panic!("claim was not admitted");
        };
        let handle = claim.handle();
        dispatcher.fetch_now(vec![claim]);
        handle.wait().await;
    }
    gate.arrived().await;
    let (healthy_id, healthy) = h.add_stub(StubProvider::new).await;
    let lease = h.provider_service.acquire(healthy_id).await.unwrap();
    let ForcedClaim::Claimed(claim) =
        h.scheduler
            .claim_forced(healthy_id, lease.generation, Instant::now())
    else {
        panic!("healthy claim was not admitted");
    };
    let handle = claim.handle();
    dispatcher.fetch_now(vec![claim]);
    let outcome = tokio::time::timeout(Duration::from_millis(300), handle.wait()).await;
    let fetches = healthy.fetch_count();
    let slow_fetches = slow.fetch_count();
    gate.open();
    cancel.cancel();
    manager.await.unwrap();

    assert!(
        outcome.is_ok(),
        "healthy refresh timed out with {fetches} provider calls"
    );
    assert_eq!(slow_fetches, FETCH_CONCURRENCY);
    assert_eq!(slow.history_fetch_count(), 1);
}
