use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::poll;
use pipedash_plugin_api::RateLimit;
use tokio::time::{
    timeout,
    Instant,
};

use super::{
    PipelineService,
    RunWatch,
};
use crate::application::test_support::plugins::STUB_BOUNDED;
use crate::application::test_support::{
    pipeline,
    stub_run,
    Gate,
    Harness,
    StubProvider,
};
use crate::domain::{
    DomainError,
    PipelineStatus,
};
use crate::domain::{
    FailureKind,
    RefreshState,
};

async fn selected(h: &Harness, gate: Option<&Arc<Gate>>) -> (i64, Arc<StubProvider>, RunWatch) {
    let (id, stub) = h
        .add_stub(|id| {
            let mut stub = StubProvider::new(id);
            if let Some(gate) = gate {
                stub = stub.run_gated(gate);
            }
            let mut run = stub_run(&pipeline(id, "web", PipelineStatus::Running).id, 7, 0);
            run.status = PipelineStatus::Running;
            stub.with_runs(vec![run])
        })
        .await;
    let mut cached = pipeline(id, "web", PipelineStatus::Running);
    cached.provider_type = "stub".to_string();
    let pipeline_id = cached.id.clone();
    h.repository
        .update_pipelines_cache(id, &[cached])
        .await
        .unwrap();
    let watch = h.pipeline_service.watch_run(pipeline_id, 7).await.unwrap();
    (id, stub, watch)
}

fn run_calls(stub: &StubProvider) -> usize {
    stub.run_fetches.load(Ordering::SeqCst)
}

async fn wait_for_claim(h: &Harness, id: i64, pending: &mut (impl Future + Unpin)) {
    timeout(Duration::from_secs(10), async {
        while !h.scheduler.view(id, Instant::now()).unwrap().in_flight {
            assert!(poll!(&mut *pending).is_pending());
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("watch did not reach scheduler admission");
}

#[tokio::test]
async fn active_quota_cooldown_makes_no_run_request() {
    let h = Harness::new().await;
    let (id, stub, mut watch) = selected(&h, None).await;
    h.scheduler.set_quota_pause(
        id,
        watch.generation,
        Some(Utc::now() + chrono::Duration::minutes(10)),
        Instant::now(),
        Utc::now(),
    );

    let result = timeout(Duration::from_secs(1), watch.next()).await.unwrap();
    assert!(matches!(result, Some(Err(DomainError::RateLimited { .. }))));
    assert!(watch.next().await.is_none());
    assert_eq!(run_calls(&stub), 0);
    assert_eq!(stub.fetch_count(), 0);
    assert!(!h.scheduler.view(id, Instant::now()).unwrap().in_flight);
}

#[tokio::test]
async fn exhausted_provider_quota_prevents_run_request() {
    let h = Harness::new().await;
    let (id, stub, mut watch) = selected(&h, None).await;
    *stub.rate_limit.lock().unwrap() = Some(RateLimit {
        limit: 5000,
        remaining: 0,
        reset_at: Utc::now() + chrono::Duration::minutes(10),
    });

    assert!(matches!(
        watch.next().await,
        Some(Err(DomainError::RateLimited { .. }))
    ));
    assert!(watch.next().await.is_none());
    assert_eq!(run_calls(&stub), 0);
    assert!(h.scheduler.paused_until(id, Utc::now()).is_some());
    assert!(!h.scheduler.view(id, Instant::now()).unwrap().in_flight);
}

#[tokio::test]
async fn run_rate_limit_blocks_another_watch_and_provider_refresh() {
    let h = Harness::new().await;
    let (id, stub, mut watch) = selected(&h, None).await;
    *stub.run_response.lock().unwrap() = Some(Err(DomainError::RateLimited {
        message: "slow down".to_string(),
        retry_after: Some(Duration::from_secs(120)),
    }));

    assert!(matches!(
        watch.next().await,
        Some(Err(DomainError::RateLimited { .. }))
    ));
    assert!(watch.next().await.is_none());
    let mut other = h
        .pipeline_service
        .watch_run(watch.pipeline_id.clone(), 7)
        .await
        .unwrap();
    let result = timeout(Duration::from_secs(1), other.next()).await.unwrap();
    assert!(matches!(result, Some(Err(DomainError::RateLimited { .. }))));
    let refreshed = h.pipeline_service.refresh_provider(id).await;
    assert_eq!(refreshed.state, RefreshState::Recent);
    assert_eq!(refreshed.kind, Some(FailureKind::RateLimited));
    assert_eq!(run_calls(&stub), 1);
    assert_eq!(stub.fetch_count(), 0);
}

#[tokio::test]
async fn provider_refresh_rate_limit_blocks_run_requests() {
    let h = Harness::new().await;
    let (id, stub, mut watch) = selected(&h, None).await;
    *stub.pipelines.lock().unwrap() = Err(DomainError::RateLimited {
        message: "slow down".to_string(),
        retry_after: Some(Duration::from_secs(120)),
    });
    h.fetch_directly(id).await;

    assert!(matches!(
        watch.next().await,
        Some(Err(DomainError::RateLimited { .. }))
    ));
    assert_eq!(run_calls(&stub), 0);
    assert_eq!(stub.fetch_count(), 1);
}

#[tokio::test]
async fn selected_run_does_not_drift_and_terminal_state_stops_fetching() {
    let h = Harness::new().await;
    let (id, stub, mut watch) = selected(&h, None).await;
    let before = h.scheduler.view(id, Instant::now()).unwrap();
    assert_eq!(watch.next().await.unwrap().unwrap().run.run_number, 7);
    let mut next_run = stub_run(&watch.pipeline_id, 8, 0);
    next_run.status = PipelineStatus::Running;
    *stub.runs.lock().unwrap() = vec![next_run, stub_run(&watch.pipeline_id, 7, 30)];
    let mut next = Box::pin(watch.next());
    assert!(poll!(&mut next).is_pending());
    assert_eq!(run_calls(&stub), 1);
    drop(next);
    watch.next_attempt = Instant::now();

    let observed = watch.next().await.unwrap().unwrap();
    assert_eq!(observed.run.run_number, 7);
    assert_eq!(observed.run.status, PipelineStatus::Success);
    assert!(watch.next().await.is_none());
    assert_eq!(run_calls(&stub), 2);
    assert_eq!(stub.fetch_count(), 0);
    let after = h.scheduler.view(id, Instant::now()).unwrap();
    assert_eq!(before.failures, after.failures);
    assert_eq!(before.last_failure, after.last_failure);
    assert_eq!(h.bus.count("pipelines-updated"), 0);
}

#[tokio::test]
async fn mismatched_provider_run_identity_is_rejected() {
    let h = Harness::new().await;
    let (_, stub, mut watch) = selected(&h, None).await;
    *stub.run_response.lock().unwrap() = Some(Ok(stub_run(&watch.pipeline_id, 8, 0)));
    assert!(matches!(
        watch.next().await,
        Some(Err(DomainError::InvalidConfig(_)))
    ));
    let mut other = h
        .pipeline_service
        .watch_run(watch.pipeline_id.clone(), 7)
        .await
        .unwrap();
    *stub.run_response.lock().unwrap() = Some(Ok(stub_run("another-pipeline", 7, 0)));
    assert!(matches!(
        other.next().await,
        Some(Err(DomainError::InvalidConfig(_)))
    ));
    assert_eq!(run_calls(&stub), 2);
}

#[tokio::test]
async fn cancelling_in_flight_run_releases_provider_lease_claim_and_permit() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, stub, mut watch) = selected(&h, Some(&gate)).await;
    let leases = Arc::strong_count(&stub);
    let pending = tokio::spawn(async move { watch.next().await });
    gate.arrived().await;
    assert!(Arc::strong_count(&stub) > leases);
    assert!(h.scheduler.view(id, Instant::now()).unwrap().in_flight);
    assert_eq!(h.pipeline_service.fetch_permits.available_permits(), 9);

    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert_eq!(Arc::strong_count(&stub), leases);
    assert!(!h.scheduler.view(id, Instant::now()).unwrap().in_flight);
    assert_eq!(h.pipeline_service.fetch_permits.available_permits(), 10);
    gate.open();
    assert_eq!(run_calls(&stub), 1);
}

#[tokio::test]
async fn blocked_watch_uses_common_fetch_permits_and_cancels_without_a_request() {
    let h = Harness::new().await;
    let (id, stub, mut watch) = selected(&h, None).await;
    let permits = h
        .pipeline_service
        .fetch_permits
        .clone()
        .acquire_many_owned(10)
        .await
        .unwrap();
    let mut pending = Box::pin(watch.next());
    wait_for_claim(&h, id, &mut pending).await;
    assert_eq!(run_calls(&stub), 0);

    drop(pending);
    assert!(!h.scheduler.view(id, Instant::now()).unwrap().in_flight);
    drop(permits);
    assert_eq!(watch.next().await.unwrap().unwrap().run.run_number, 7);
    assert_eq!(run_calls(&stub), 1);
}

#[tokio::test]
async fn replacing_credentials_rejects_an_in_flight_result() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, old, mut watch) = selected(&h, Some(&gate)).await;
    let replacement = h.stubs.queue(StubProvider::new(id));
    let pending = tokio::spawn(async move { watch.next().await });
    gate.arrived().await;
    let mut config = h.config("replacement", 30);
    config.id = Some(id);
    config.token = "new-token".to_string();
    h.provider_service
        .update_provider(id, config)
        .await
        .unwrap();
    gate.open();

    assert!(matches!(
        pending.await.unwrap(),
        Some(Err(DomainError::ConcurrentModification(_)))
    ));
    assert_eq!(run_calls(&old), 1);
    assert_eq!(run_calls(&replacement), 0);
    assert_eq!(replacement.fetch_count(), 0);
}

#[tokio::test]
async fn replacing_credentials_before_next_makes_no_old_run_request() {
    let h = Harness::new().await;
    let (id, old, mut watch) = selected(&h, None).await;
    let replacement = h.stubs.queue(StubProvider::new(id));
    let mut config = h.config("replacement", 30);
    config.id = Some(id);
    h.provider_service
        .update_provider(id, config)
        .await
        .unwrap();

    assert!(matches!(
        watch.next().await,
        Some(Err(DomainError::ConcurrentModification(_)))
    ));
    assert_eq!(run_calls(&old), 0);
    assert_eq!(run_calls(&replacement), 0);
}

#[tokio::test]
async fn replaced_credentials_are_not_used_after_waiting_for_a_fetch_permit() {
    let h = Harness::new().await;
    let (id, old, mut watch) = selected(&h, None).await;
    let permits = h
        .pipeline_service
        .fetch_permits
        .clone()
        .acquire_many_owned(10)
        .await
        .unwrap();
    let mut pending = Box::pin(watch.next());
    wait_for_claim(&h, id, &mut pending).await;
    let replacement = h.stubs.queue(StubProvider::new(id));
    let mut config = h.config("replacement", 30);
    config.id = Some(id);
    h.provider_service
        .update_provider(id, config)
        .await
        .unwrap();
    drop(permits);

    assert!(matches!(
        pending.await,
        Some(Err(DomainError::ConcurrentModification(_)))
    ));
    assert_eq!(run_calls(&old), 0);
    assert_eq!(run_calls(&replacement), 0);
}

#[tokio::test]
async fn waiting_for_a_fetch_permit_ends_at_the_core_deadline() {
    let h = Harness::new().await;
    let (id, stub, mut watch) = selected(&h, None).await;
    let permits = h
        .pipeline_service
        .fetch_permits
        .clone()
        .acquire_many_owned(10)
        .await
        .unwrap();
    let mut pending = Box::pin(watch.next());
    wait_for_claim(&h, id, &mut pending).await;
    tokio::time::pause();
    tokio::time::advance(PipelineService::REFRESH_DEADLINE + Duration::from_secs(1)).await;

    assert!(matches!(pending.await, Some(Err(DomainError::Timeout(_)))));
    assert!(watch.next().await.is_none());
    assert_eq!(run_calls(&stub), 0);
    assert!(!h.scheduler.view(id, Instant::now()).unwrap().in_flight);
    drop(permits);
    assert_eq!(h.pipeline_service.fetch_permits.available_permits(), 10);
}

async fn cached_bounded_run(
    h: &Harness, status: PipelineStatus, archivable: bool,
) -> (Arc<StubProvider>, RunWatch) {
    let (id, stub) = h
        .add_stub(|id| StubProvider::new(id).bounded_history())
        .await;
    let mut config = h.config("Stub", 30);
    config.id = Some(id);
    config.provider_type = STUB_BOUNDED.to_string();
    h.repository.update_provider(id, &config).await.unwrap();
    let mut cached = pipeline(id, "web", PipelineStatus::Running);
    cached.id = format!("{STUB_BOUNDED}__{id}__HelmRelease__apps__web");
    cached.provider_type = STUB_BOUNDED.to_string();
    let pipeline_id = cached.id.clone();
    h.repository
        .update_pipelines_cache(id, &[cached])
        .await
        .unwrap();
    let mut run = stub_run(&pipeline_id, 7, 30);
    run.status = status;
    run.metadata
        .insert("archivable".to_string(), serde_json::json!(archivable));
    h.repository
        .cache_run_history(&pipeline_id, &[run])
        .await
        .unwrap();
    *stub.run_response.lock().unwrap() = Some(Err(DomainError::PipelineNotFound(
        "remote execution missing".to_string(),
    )));
    let watch = h.pipeline_service.watch_run(pipeline_id, 7).await.unwrap();
    (stub, watch)
}

#[tokio::test]
async fn missing_bounded_execution_uses_the_same_archived_terminal_fallback() {
    let h = Harness::new().await;
    let (stub, mut watch) = cached_bounded_run(&h, PipelineStatus::Success, true).await;
    let details = h
        .pipeline_service
        .fetch_run_details(&watch.pipeline_id, 7)
        .await
        .unwrap();
    assert_eq!(details.metadata["archived"], serde_json::json!(true));

    let observed = watch.next().await.unwrap().unwrap();
    assert_eq!(observed.run.run_number, 7);
    assert_eq!(observed.run.status, PipelineStatus::Success);
    assert_eq!(observed.run.metadata["archived"], serde_json::json!(true));
    assert!(watch.next().await.is_none());
    assert_eq!(run_calls(&stub), 2);
    assert_eq!(stub.fetch_count(), 0);
}

#[tokio::test]
async fn unarchivable_bounded_cache_keeps_the_remote_lookup_error() {
    for (status, archivable) in [
        (PipelineStatus::Running, true),
        (PipelineStatus::Pending, true),
        (PipelineStatus::Success, false),
    ] {
        let h = Harness::new().await;
        let (stub, mut watch) = cached_bounded_run(&h, status, archivable).await;

        assert!(matches!(
            watch.next().await,
            Some(Err(DomainError::PipelineNotFound(message)))
                if message == "remote execution missing"
        ));
        assert!(watch.next().await.is_none());
        assert_eq!(run_calls(&stub), 1);
        assert_eq!(stub.fetch_count(), 0);
    }
}
