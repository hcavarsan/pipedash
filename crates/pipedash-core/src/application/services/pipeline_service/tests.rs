use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::RunHistoryOrder;
use serde_json::json;
use tokio::time::Instant;

use super::fetch::needing_attention;
use super::history::archived_run;
use super::overdue::{
    overdue_run,
    typical_duration,
};
use super::{
    FETCH_CONCURRENCY,
    READ_ONLY_PROVIDER,
};
use crate::application::fetch_scheduler::{
    ClaimKind,
    FetchResult,
};
use crate::application::refresh_manager::RefreshMode;
use crate::application::services::health_service::HealthInput;
use crate::application::test_support::plugins::{
    STUB,
    STUB_BOUNDED,
    STUB_FORCE,
    STUB_RETAINING,
};
use crate::application::test_support::{
    pipeline,
    stub_run,
    Gate,
    Harness,
    StubProvider,
};
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RefreshState,
    RunHistoryQuery,
    TriggerParams,
};
use crate::domain::{
    FetchStatus,
    ProviderConfig,
};
use crate::event::CoreEvent;

fn history_fixture() -> (Pipeline, PipelineRun) {
    let pipeline = serde_json::from_value(json!({
        "id": "stub-bounded__1__HelmRelease__apps__web", "provider_id": 1,
        "provider_type": "stub-bounded", "name": "web", "status": "success",
        "repository": "apps/charts", "last_updated": "2026-10-21T00:00:00Z",
        "metadata": { "uid": "resource-1" }
    }))
    .unwrap();
    let run = serde_json::from_value(json!({
        "id": "release-1", "pipeline_id": "stub-bounded__1__HelmRelease__apps__web",
        "run_number": 1, "status": "success", "started_at": "2026-10-20T00:00:00Z",
        "logs_url": "", "metadata": { "archivable": true, "release_version": 1 }
    }))
    .unwrap();
    (pipeline, run)
}

#[tokio::test]
async fn action_invalidation_keeps_archived_history_and_details() {
    let h = Harness::new().await;
    let (provider_id, stub) = h
        .add_stub(|id| {
            StubProvider::new(id)
                .bounded_history()
                .retaining_run_history()
        })
        .await;
    let (mut pipeline, archived_run) = history_fixture();
    pipeline.provider_id = provider_id;
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    let service = &h.pipeline_service;
    service
        .merge_run_cache(
            stub.as_ref(),
            &pipeline,
            std::slice::from_ref(&archived_run),
        )
        .await
        .unwrap();
    service
        .merge_run_cache(stub.as_ref(), &pipeline, &[])
        .await
        .unwrap();

    service.invalidate_run_cache(&pipeline.id).await;

    let history = service
        .query_run_history(&pipeline.id, RunHistoryQuery::page(1, 20))
        .await
        .unwrap();
    assert_eq!(history.total_count, 1);
    assert_eq!(history.runs[0].id, archived_run.id);
    assert_eq!(history.runs[0].metadata["archived"], json!(true));
    let details = service
        .fetch_run_details(&pipeline.id, archived_run.run_number)
        .await
        .unwrap();
    assert_eq!(details.id, archived_run.id);
    assert_eq!(details.metadata["archived"], json!(true));
    service.settle().await;
}

#[tokio::test]
async fn action_invalidation_clears_history_for_non_retaining_providers() {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            StubProvider::new(id).with_runs(vec![stub_run(&format!("stub__{id}__web"), 2, 60)])
        })
        .await;
    let pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    let old_run = stub_run(&pipeline.id, 1, 60);
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    h.repository
        .cache_run_history(&pipeline.id, &[old_run])
        .await
        .unwrap();

    h.pipeline_service.invalidate_run_cache(&pipeline.id).await;

    assert_eq!(
        h.repository
            .get_cached_run_count(&pipeline.id)
            .await
            .unwrap(),
        0
    );
    let history = h
        .pipeline_service
        .query_run_history(&pipeline.id, RunHistoryQuery::page(1, 20))
        .await
        .unwrap();
    assert_eq!(history.runs[0].run_number, 2);
    h.pipeline_service.settle().await;
}

#[test]
fn retained_native_history_is_marked_as_archived() {
    let (pipeline, run) = history_fixture();
    let provider = StubProvider::new(1).bounded_history();
    let archived = archived_run(&provider, &pipeline, run).unwrap();
    assert_eq!(archived.id, "release-1");
    assert_eq!(archived.metadata["archived"], json!(true));
    assert_eq!(archived.metadata["release_version"], json!(1));
}

#[test]
fn runs_the_plugin_does_not_mark_archivable_keep_their_remote_error() {
    let (pipeline, mut run) = history_fixture();
    let provider = StubProvider::new(1).bounded_history();
    run.metadata.remove("archivable");
    assert!(archived_run(&provider, &pipeline, run).is_none());
}

#[test]
fn unfinished_history_is_not_presented_as_a_final_state() {
    let (pipeline, mut run) = history_fixture();
    let provider = StubProvider::new(1).bounded_history();
    run.status = PipelineStatus::Running;
    assert!(archived_run(&provider, &pipeline, run).is_none());
}

#[test]
fn providers_without_archive_support_never_archive() {
    let (pipeline, run) = history_fixture();
    assert!(archived_run(&StubProvider::new(1), &pipeline, run).is_none());
}

fn run(number: i64, status: &str, started_minutes_ago: i64, minutes: Option<i64>) -> PipelineRun {
    serde_json::from_value(json!({
        "id": number.to_string(), "pipeline_id": "stub__1__build", "run_number": number,
        "status": status, "started_at": Utc::now() - chrono::Duration::minutes(started_minutes_ago),
        "duration_seconds": minutes.map(|m| m * 60), "logs_url": ""
    }))
    .unwrap()
}

fn ten_minute_history(running_for_minutes: i64) -> Vec<PipelineRun> {
    vec![
        run(5, "running", running_for_minutes, None),
        run(4, "success", 120, Some(10)),
        run(3, "failed", 180, Some(90)),
        run(2, "success", 240, Some(9)),
        run(1, "success", 300, Some(11)),
    ]
}

#[test]
fn only_a_pipeline_that_newly_fails_or_degrades_needs_attention() {
    let at = |minutes: i64| Some(Utc::now() + chrono::Duration::minutes(minutes));
    let make = |status, last_run| {
        let mut pipeline = pipeline(1, "web", status);
        pipeline.last_run = last_run;
        pipeline
    };
    let first_run = at(0);
    let second_run = at(5);
    let failures = |old: Option<Pipeline>, new: Pipeline| {
        let old: Vec<Pipeline> = old.into_iter().collect();
        let old_by_id: HashMap<&str, &Pipeline> = old.iter().map(|p| (p.id.as_str(), p)).collect();

        needing_attention(&old_by_id, &[new]).len()
    };

    assert_eq!(
        failures(
            Some(make(PipelineStatus::Success, first_run)),
            make(PipelineStatus::Failed, second_run)
        ),
        1
    );
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Running, first_run)),
            make(PipelineStatus::Failed, first_run)
        ),
        1
    );
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Failed, first_run)),
            make(PipelineStatus::Failed, second_run)
        ),
        1
    );
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Failed, first_run)),
            make(PipelineStatus::Failed, first_run)
        ),
        0
    );
    assert_eq!(failures(None, make(PipelineStatus::Failed, first_run)), 0);
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Failed, first_run)),
            make(PipelineStatus::Success, second_run)
        ),
        0
    );
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Success, first_run)),
            make(PipelineStatus::Cancelled, second_run)
        ),
        0
    );
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Success, first_run)),
            make(PipelineStatus::Degraded, first_run)
        ),
        1
    );
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Degraded, first_run)),
            make(PipelineStatus::Failed, first_run)
        ),
        1
    );
    assert_eq!(
        failures(
            Some(make(PipelineStatus::Degraded, first_run)),
            make(PipelineStatus::Degraded, first_run)
        ),
        0
    );
}

#[test]
fn a_run_is_overdue_past_three_times_the_median_success() {
    let now = Utc::now();

    assert_eq!(
        overdue_run(&ten_minute_history(45), now),
        Some("5".to_string())
    );
    assert_eq!(overdue_run(&ten_minute_history(25), now), None);
    assert_eq!(overdue_run(&ten_minute_history(45)[..3], now), None);
    assert_eq!(overdue_run(&ten_minute_history(45)[1..], now), None);
}

#[test]
fn typical_duration_is_the_median_of_positive_successful_durations() {
    let history = |minutes: &[Option<i64>]| -> Vec<PipelineRun> {
        minutes
            .iter()
            .enumerate()
            .map(|(i, m)| run(i as i64 + 1, "success", 60, *m))
            .collect()
    };

    assert_eq!(typical_duration(&ten_minute_history(45)), Some(600));
    assert_eq!(
        typical_duration(&history(&[Some(10), Some(20), Some(30), Some(40)])),
        Some(1500)
    );
    assert_eq!(
        typical_duration(&history(&[Some(10), None, Some(0), Some(20)])),
        None
    );
    assert_eq!(
        typical_duration(&history(&[Some(10), None, Some(0), Some(20), Some(30)])),
        Some(1200)
    );
    assert_eq!(typical_duration(&ten_minute_history(45)[..3]), None);
}

#[tokio::test]
async fn refresh_flags_a_running_pipeline_far_past_its_usual_duration() {
    let h = Harness::new().await;
    let repository = &h.repository;
    let provider_id = repository
        .add_provider(&ProviderConfig {
            id: None,
            name: "stub".into(),
            provider_type: "stub".into(),
            token: "token".into(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        })
        .await
        .unwrap();
    let pipeline: Pipeline = serde_json::from_value(json!({
        "id": "stub__1__build", "provider_id": provider_id, "provider_type": "stub",
        "name": "build", "status": "running", "repository": "build",
        "last_updated": Utc::now()
    }))
    .unwrap();
    repository
        .update_pipelines_cache(provider_id, &[pipeline])
        .await
        .unwrap();
    repository
        .cache_run_history("stub__1__build", &ten_minute_history(45))
        .await
        .unwrap();
    let service = &h.pipeline_service;

    let before = service.get_cached_pipelines(None).await.unwrap();
    let changed = service.update_overdue_pipelines(provider_id).await;
    let unchanged = service.update_overdue_pipelines(provider_id).await;
    let after = service.get_cached_pipelines(None).await.unwrap();
    repository
        .clear_cached_run_history("stub__1__build")
        .await
        .unwrap();
    let after_failed_lookup = service.update_overdue_pipelines(provider_id).await;
    let unknown = service.get_cached_pipelines(None).await.unwrap();
    repository
        .cache_run_history("stub__1__build", &ten_minute_history(45))
        .await
        .unwrap();
    service.update_overdue_pipelines(provider_id).await;
    let again = service.get_cached_pipelines(None).await.unwrap();

    assert_eq!(before[0].overdue_run, None);
    assert_eq!(before[0].typical_duration_seconds, None);
    assert_eq!(after[0].typical_duration_seconds, Some(600));
    assert_eq!(unknown[0].typical_duration_seconds, None);
    assert_eq!(again[0].typical_duration_seconds, Some(600));
    assert!(changed);
    assert!(!unchanged);
    assert_eq!(after[0].overdue_run.as_deref(), Some("5"));
    assert!(after_failed_lookup);
    assert_eq!(unknown[0].overdue_run, None);
    assert_eq!(again[0].overdue_run.as_deref(), Some("5"));
}

#[tokio::test]
async fn overdue_timing_fetches_uncached_pipelines_in_one_batch() {
    let h = Harness::new().await;
    let (provider_id, stub) = h
        .add_stub(|id| {
            StubProvider::new(id)
                .batching()
                .with_runs(ten_minute_history(45))
        })
        .await;
    let running: Vec<Pipeline> = ["build", "test"]
        .iter()
        .map(|name| {
            serde_json::from_value(json!({
                "id": format!("stub__{provider_id}__{name}"), "provider_id": provider_id,
                "provider_type": "stub", "name": name, "status": "running",
                "repository": name, "last_updated": Utc::now()
            }))
            .unwrap()
        })
        .collect();
    h.repository
        .update_pipelines_cache(provider_id, &running)
        .await
        .unwrap();

    let changed = h
        .pipeline_service
        .update_overdue_pipelines(provider_id)
        .await;
    let cached = h
        .pipeline_service
        .get_cached_pipelines(Some(provider_id))
        .await
        .unwrap();

    assert!(changed);
    assert_eq!(stub.batch_fetch_count(), 1);
    assert_eq!(stub.history_fetch_count(), 0);
    for pipeline in &cached {
        assert_eq!(pipeline.typical_duration_seconds, Some(600));
        assert_eq!(pipeline.overdue_run.as_deref(), Some("5"));
        assert!(
            h.repository
                .get_cached_run_count(&pipeline.id)
                .await
                .unwrap()
                > 0
        );
    }
}

#[tokio::test]
async fn no_auth_providers_refuse_actions_from_their_saved_config() {
    let h = Harness::new().await;
    let (public_id, _) = h.add_stub(StubProvider::new).await;
    let (signed_id, _) = h.add_stub(StubProvider::new).await;
    let mut public_config = h.repository.get_provider(public_id).await.unwrap();
    public_config
        .config
        .insert("auth_mode".to_string(), "none".to_string());
    h.repository
        .update_provider(public_id, &public_config)
        .await
        .unwrap();
    for provider_id in [public_id, signed_id] {
        let stale: Pipeline = serde_json::from_value(json!({
            "id": format!("stub__{provider_id}__build"), "provider_id": provider_id,
            "provider_type": "stub", "name": "build", "status": "running",
            "repository": "build", "last_updated": Utc::now(),
            "metadata": { "read_only": true }
        }))
        .unwrap();
        h.repository
            .update_pipelines_cache(provider_id, &[stale])
            .await
            .unwrap();
    }
    let public = format!("stub__{public_id}__build");
    let signed = format!("stub__{signed_id}__build");
    let refused = |result: DomainResult<()>| matches!(result, Err(DomainError::NotSupported(message)) if message == READ_ONLY_PROVIDER);
    let service = &h.pipeline_service;

    let triggered = service
        .trigger_pipeline(TriggerParams {
            workflow_id: public.clone(),
            inputs: None,
        })
        .await
        .map(|_| ());

    assert!(refused(triggered));
    assert!(refused(service.cancel_run(&public, 1).await));
    assert!(refused(service.force_cancel_run(&public, 1).await));
    assert!(refused(service.rerun_run(&public, 1).await.map(|_| ())));
    assert!(refused(service.rerun_failed_jobs(&public, 1).await));
    assert!(refused(
        service
            .review_run(&public, 1, pipedash_plugin_api::ReviewDecision::Approve)
            .await
    ));
    assert!(refused(service.enable_pipeline(&public).await));
    assert!(!refused(service.cancel_run(&signed, 1).await));
}

#[tokio::test]
async fn force_cancellation_only_invalidates_history_after_provider_acceptance() {
    let h = Harness::new().await;
    let (provider_id, stub) = h
        .add_stub(|id| {
            let mut provider = StubProvider::new(id);
            provider.capabilities.force_cancel = true;
            provider
        })
        .await;
    let mut target = pipeline(provider_id, "build", PipelineStatus::Running);
    target.provider_type = STUB_FORCE.to_string();
    let pipeline_id = target.id.clone();
    h.repository
        .update_pipelines_cache(provider_id, &[target])
        .await
        .unwrap();
    let mut cached_run = run(7, "running", 5, None);
    cached_run.pipeline_id.clone_from(&pipeline_id);
    h.repository
        .cache_run_history(&pipeline_id, std::slice::from_ref(&cached_run))
        .await
        .unwrap();
    *stub.run_response.lock().unwrap() = Some(Ok(cached_run));
    stub.force_cancel_rejected.store(true, Ordering::SeqCst);

    let rejected = h.pipeline_service.force_cancel_run(&pipeline_id, 7).await;
    assert!(
        matches!(rejected, Err(DomainError::PermissionDenied(_))),
        "{rejected:?}"
    );
    assert_eq!(
        h.repository
            .get_cached_run_count(&pipeline_id)
            .await
            .unwrap(),
        1
    );
    assert_eq!(h.bus.count("run-cancelled"), 0);

    stub.force_cancel_rejected.store(false, Ordering::SeqCst);
    h.pipeline_service
        .force_cancel_run(&pipeline_id, 7)
        .await
        .unwrap();
    assert_eq!(
        h.repository
            .get_cached_run_count(&pipeline_id)
            .await
            .unwrap(),
        0
    );
    assert!(h.bus.events().iter().any(|event| {
        matches!(event, CoreEvent::RunCancelled { pipeline_id: id } if id == &pipeline_id)
    }));
}

#[tokio::test]
async fn run_actions_are_checked_against_fresh_run_details_not_the_cache() {
    let h = Harness::new().await;
    let (provider_id, stub) = h
        .add_stub(|id| {
            let mut provider = StubProvider::new(id);
            provider.capabilities.force_cancel = true;
            provider
        })
        .await;
    let mut target = pipeline(provider_id, "build", PipelineStatus::Running);
    target.provider_type = STUB_FORCE.to_string();
    let pipeline_id = target.id.clone();
    h.repository
        .update_pipelines_cache(provider_id, &[target])
        .await
        .unwrap();
    let mut stale = run(7, "success", 5, None);
    stale.pipeline_id.clone_from(&pipeline_id);
    h.repository
        .cache_run_history(&pipeline_id, &[stale])
        .await
        .unwrap();
    let mut fresh = run(7, "running", 5, None);
    fresh.pipeline_id.clone_from(&pipeline_id);
    *stub.run_response.lock().unwrap() = Some(Ok(fresh));

    h.pipeline_service
        .force_cancel_run(&pipeline_id, 7)
        .await
        .unwrap();
    assert!(h.bus.events().iter().any(|event| {
        matches!(event, CoreEvent::RunCancelled { pipeline_id: id } if id == &pipeline_id)
    }));
}

#[tokio::test]
async fn force_cancellation_requires_provider_support_and_a_known_pipeline() {
    let h = Harness::new().await;
    let (provider_id, _) = h.add_stub(StubProvider::new).await;
    let target = pipeline(provider_id, "build", PipelineStatus::Running);
    let pipeline_id = target.id.clone();
    h.repository
        .update_pipelines_cache(provider_id, &[target])
        .await
        .unwrap();

    assert!(matches!(
        h.pipeline_service.force_cancel_run(&pipeline_id, 7).await,
        Err(DomainError::NotSupported(_))
    ));
    assert!(matches!(
        h.pipeline_service.force_cancel_run("missing", 7).await,
        Err(DomainError::PipelineNotFound(_))
    ));
    assert_eq!(h.bus.count("run-cancelled"), 0);
}

#[tokio::test]
async fn a_hanging_provider_does_not_lose_another_providers_result() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (hanging, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
    let (healthy, _) = h.add_stub(StubProvider::new).await;
    h.start().await;

    let results = h
        .pipeline_service
        .fetch_now(&[hanging, healthy], Duration::from_secs(2))
        .await;
    let summaries = h.provider_service.list_providers().await.unwrap();
    let summary = |id| summaries.iter().find(|summary| summary.id == id).unwrap();

    assert_eq!(results[0].state, RefreshState::Pending);
    assert_eq!(results[1].state, RefreshState::Ok);
    assert_eq!(h.cached_names(healthy).await, ["web"]);
    assert!(h.cached_names(hanging).await.is_empty());
    assert!(matches!(
        summary(healthy).last_fetch_status,
        FetchStatus::Success
    ));
    assert!(matches!(
        summary(hanging).last_fetch_status,
        FetchStatus::Never
    ));
    assert!(summary(hanging).health.in_flight);

    gate.open();
    h.bus
            .wait_for("the late result", |events| {
                events.iter().any(|event| {
                    matches!(event, CoreEvent::PipelinesUpdated { provider_id, .. } if *provider_id == hanging)
                })
            })
            .await;
    assert_eq!(h.cached_names(hanging).await, ["web"]);
}

#[tokio::test]
async fn a_failed_cache_write_is_never_reported_as_a_successful_fetch() {
    let h = Harness::new().await;
    let (id, _) = h.add_stub(StubProvider::new).await;
    sqlx::query("DROP TABLE pipelines_cache")
        .execute(&h.pool)
        .await
        .unwrap();

    let result = h.fetch_directly(id).await;
    let fetch = h.repository.get_provider_fetch_status(id).await.unwrap();

    assert!(matches!(result, FetchResult::Committed(Err(_))));
    assert_eq!(fetch.status, "error");
    assert!(fetch.error.is_some());
    assert_eq!(fetch.succeeded_at, None);
    assert_eq!(h.bus.count("pipelines-updated"), 0);
}

#[tokio::test]
async fn a_manual_refresh_during_a_scheduled_attempt_joins_it() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, stub) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
    h.provider_service.acquire(id).await.unwrap();
    let claim = h.pipeline_service.claim_due(Instant::now()).await.remove(0);
    let service = Arc::clone(&h.pipeline_service);
    let scheduled =
        tokio::spawn(async move { service.fetch_provider(claim, ClaimKind::Scheduled).await });
    gate.arrived().await;

    let service = Arc::clone(&h.pipeline_service);
    let manual =
        tokio::spawn(async move { service.fetch_now(&[id], Duration::from_secs(5)).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    gate.open();

    let results = manual.await.unwrap();
    assert!(matches!(
        scheduled.await.unwrap(),
        FetchResult::Committed(Ok(1))
    ));
    assert_eq!(results[0].state, RefreshState::Ok);
    assert_eq!(stub.fetch_count(), 1);
}

#[tokio::test]
async fn going_idle_stops_a_scheduled_fetch_waiting_for_a_permit() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    h.provider_service.acquire(id).await.unwrap();
    let permits = Arc::clone(&h.pipeline_service.fetch_permits)
        .acquire_many_owned(FETCH_CONCURRENCY as u32)
        .await
        .unwrap();
    let claim = h.pipeline_service.claim_due(Instant::now()).await.remove(0);
    let service = Arc::clone(&h.pipeline_service);
    let waiting =
        tokio::spawn(async move { service.fetch_provider(claim, ClaimKind::Scheduled).await });
    tokio::time::sleep(Duration::from_millis(100)).await;

    h.refresh_manager.set_mode(RefreshMode::Idle);
    drop(permits);

    assert!(matches!(waiting.await.unwrap(), FetchResult::Cancelled));
    assert_eq!(stub.fetch_count(), 0);
    let view = h.scheduler.view(id, Instant::now()).unwrap();
    assert!(!view.in_flight);
    assert_eq!(view.failures, 0);
    assert_eq!(view.next_attempt_in, Duration::ZERO);
    h.start().await;
    let results = h
        .pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    assert_eq!(results[0].state, RefreshState::Ok);
    assert_eq!(stub.fetch_count(), 1);
}

#[tokio::test]
async fn a_second_refresh_all_joins_the_running_one() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (_, stub) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
    h.start().await;
    let first = tokio::spawn({
        let service = Arc::clone(&h.pipeline_service);
        async move { service.refresh_all().await }
    });
    gate.arrived().await;
    let second = tokio::spawn({
        let service = Arc::clone(&h.pipeline_service);
        async move { service.refresh_all().await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    gate.open();

    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();
    let third = h.pipeline_service.refresh_all().await.unwrap();

    assert_eq!(first, second);
    assert_eq!(first[0].state, RefreshState::Ok);
    assert_eq!(third[0].state, RefreshState::Recent);
    assert_eq!(stub.fetch_count(), 1);
}

#[tokio::test]
async fn refresh_all_keeps_the_run_history_of_plugins_that_retain_it() {
    let h = Harness::new().await;
    h.start().await;
    let mut ids = Vec::new();
    for provider_type in [STUB, STUB_RETAINING] {
        let mut config = h.config(provider_type, 30);
        config.provider_type = provider_type.to_string();
        let id = h.repository.add_provider(&config).await.unwrap();
        h.stubs.queue(StubProvider::new(id));
        let pipeline_id = format!("stub__{id}__web");
        h.repository
            .update_pipelines_cache(id, &[pipeline(id, "web", PipelineStatus::Success)])
            .await
            .unwrap();
        h.repository
            .cache_run_history(&pipeline_id, &[stub_run(&pipeline_id, 1, 60)])
            .await
            .unwrap();
        ids.push(pipeline_id);
    }

    let results = h.pipeline_service.refresh_all().await.unwrap();

    assert!(results
        .iter()
        .all(|result| result.state == RefreshState::Ok));
    assert_eq!(h.repository.get_cached_run_count(&ids[0]).await.unwrap(), 0);
    assert_eq!(h.repository.get_cached_run_count(&ids[1]).await.unwrap(), 1);
}

#[tokio::test]
async fn a_changed_pipeline_clears_its_run_cache_unless_the_plugin_retains_history() {
    let h = Harness::new().await;
    let (plain, plain_stub) = h.add_stub(StubProvider::new).await;
    let (retaining, retaining_stub) = h
        .add_stub(|id| StubProvider::new(id).retaining_run_history())
        .await;
    let mut cached = Vec::new();
    for (id, stub) in [(plain, &plain_stub), (retaining, &retaining_stub)] {
        h.fetch_directly(id).await;
        let pipeline_id = format!("stub__{id}__web");
        h.repository
            .cache_run_history(&pipeline_id, &[stub_run(&pipeline_id, 1, 60)])
            .await
            .unwrap();
        stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);
        h.fetch_directly(id).await;
        cached.push(
            h.repository
                .get_cached_run_count(&pipeline_id)
                .await
                .unwrap(),
        );
    }

    assert_eq!(cached, [0, 1]);
}

#[tokio::test]
async fn reselected_pipelines_load_new_runs_without_clearing_history_manually() {
    let h = Harness::new().await;
    let (id, stub) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            StubProvider::new(id).with_runs(
                (1..=22)
                    .rev()
                    .map(|n| stub_run(&pipeline_id, n, 60))
                    .collect(),
            )
        })
        .await;
    let pipeline_id = format!("stub__{id}__web");
    h.fetch_directly(id).await;
    let cached_runs: Vec<_> = (1..=21)
        .rev()
        .map(|n| stub_run(&pipeline_id, n, 60))
        .collect();
    h.repository
        .cache_run_history(&pipeline_id, &cached_runs)
        .await
        .unwrap();
    let initial = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(1, 20))
        .await
        .unwrap();
    assert_eq!(initial.runs[0].run_number, 21);
    assert_eq!(initial.total_count, 21);

    stub.set_pipelines(vec![pipeline(id, "api", PipelineStatus::Success)]);
    h.fetch_directly(id).await;
    assert_eq!(h.cached_names(id).await, ["api"]);
    stub.set_pipelines(vec![
        pipeline(id, "web", PipelineStatus::Success),
        pipeline(id, "api", PipelineStatus::Success),
    ]);
    h.fetch_directly(id).await;

    let restored = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(1, 20))
        .await
        .unwrap();
    assert_eq!(restored.runs[0].run_number, 22);
    assert_eq!(restored.total_count, 22);
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn reappearing_pipelines_keep_retained_history() {
    let h = Harness::new().await;
    let (id, stub) = h
        .add_stub(|id| StubProvider::new(id).retaining_run_history())
        .await;
    h.fetch_directly(id).await;
    let pipeline_id = format!("stub__{id}__web");
    let retained = stub_run(&pipeline_id, 1, 60);
    h.repository
        .cache_run_history(&pipeline_id, std::slice::from_ref(&retained))
        .await
        .unwrap();
    stub.set_pipelines(Vec::new());
    h.fetch_directly(id).await;
    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Success)]);
    h.fetch_directly(id).await;

    let history = h
        .repository
        .get_cached_run_history(&pipeline_id, 20, RunHistoryOrder::RunNumber)
        .await
        .unwrap();
    assert_eq!(history, vec![retained]);
}

#[tokio::test]
async fn a_fetch_publishes_its_provider_and_only_reports_real_changes() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    let web = format!("stub__{id}__web");
    let api = format!("stub__{id}__api");

    h.fetch_directly(id).await;
    h.fetch_directly(id).await;
    stub.set_pipelines(vec![
        pipeline(id, "web", PipelineStatus::Failed),
        pipeline(id, "api", PipelineStatus::Success),
    ]);
    h.fetch_directly(id).await;
    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);
    h.fetch_directly(id).await;

    let events = h.bus.events();
    let changed: Vec<Vec<String>> = events
        .iter()
        .filter_map(|event| match event {
            CoreEvent::PipelineStatusChanged { pipeline_ids } => {
                let mut ids = pipeline_ids.clone();
                ids.sort();
                Some(ids)
            }
            _ => None,
        })
        .collect();
    let updated: Vec<i64> = events
        .iter()
        .filter_map(|event| match event {
            CoreEvent::PipelinesUpdated { provider_id, .. } => Some(*provider_id),
            _ => None,
        })
        .collect();

    assert_eq!(updated, vec![id; 4]);
    assert_eq!(h.bus.count("pipeline-cache-invalidated"), 4);
    assert_eq!(
        changed,
        vec![vec![web.clone()], vec![api.clone(), web.clone()], vec![api]]
    );
}

#[tokio::test]
async fn a_pipeline_that_newly_fails_is_reported_once_per_failure() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    let web = format!("stub__{id}__web");
    let reported = || -> Vec<Vec<String>> {
        h.bus
            .events()
            .iter()
            .filter_map(|event| match event {
                CoreEvent::PipelinesFailed { pipelines } => {
                    Some(pipelines.iter().map(|p| p.id.clone()).collect())
                }
                _ => None,
            })
            .collect()
    };

    h.fetch_directly(id).await;
    assert!(reported().is_empty());

    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);
    h.fetch_directly(id).await;
    h.fetch_directly(id).await;
    h.fetch_directly(id).await;
    assert_eq!(reported(), vec![vec![web.clone()]]);

    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Running)]);
    h.fetch_directly(id).await;
    assert_eq!(reported().len(), 1);

    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);
    h.fetch_directly(id).await;
    assert_eq!(reported(), vec![vec![web.clone()], vec![web]]);
}

#[tokio::test]
async fn a_run_that_finishes_is_reported_once() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    let finished = || -> Vec<Vec<(String, PipelineStatus)>> {
        h.bus
            .events()
            .iter()
            .filter_map(|event| match event {
                CoreEvent::PipelinesFinished { pipelines } => Some(
                    pipelines
                        .iter()
                        .map(|p| (p.name.clone(), p.status.clone()))
                        .collect(),
                ),
                _ => None,
            })
            .collect()
    };

    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Running)]);
    h.fetch_directly(id).await;
    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Success)]);
    h.fetch_directly(id).await;
    h.fetch_directly(id).await;
    assert_eq!(
        finished(),
        vec![vec![("web".to_owned(), PipelineStatus::Success)]]
    );

    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Pending)]);
    h.fetch_directly(id).await;
    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);
    h.fetch_directly(id).await;
    assert_eq!(finished().len(), 2);
    assert_eq!(
        finished()[1],
        vec![("web".to_owned(), PipelineStatus::Failed)]
    );
}

#[tokio::test]
async fn a_pipeline_seen_for_the_first_time_is_not_reported_as_failed() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);

    h.fetch_directly(id).await;
    h.fetch_directly(id).await;

    assert_eq!(h.bus.count("pipelines-failed"), 0);
}

#[tokio::test]
async fn a_failed_fetch_keeps_the_cache_and_records_the_failure() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    h.fetch_directly(id).await;
    h.health_inputs();
    *stub.pipelines.lock().unwrap() = Err(DomainError::RateLimited {
        message: "slow down".to_string(),
        retry_after: Some(Duration::from_secs(120)),
    });

    let result = h.fetch_directly(id).await;
    let view = h.scheduler.view(id, Instant::now()).unwrap();
    let summary = &h.provider_service.list_providers().await.unwrap()[0];
    let inputs = h.health_inputs();

    assert!(matches!(
        result,
        FetchResult::Committed(Err(DomainError::RateLimited { .. }))
    ));
    assert_eq!(h.cached_names(id).await, ["web"]);
    assert_eq!(view.failures, 1);
    assert!(view.next_attempt_in >= Duration::from_secs(119));
    assert!(matches!(summary.last_fetch_status, FetchStatus::Error));
    assert_eq!(
        summary.last_fetch_error_kind,
        Some(crate::domain::FailureKind::RateLimited)
    );
    assert!(summary.health.next_attempt_at.is_some());
    assert!(matches!(
        inputs.as_slice(),
        [HealthInput::FetchOutcome {
            kind: Some(crate::domain::FailureKind::RateLimited),
            ..
        }]
    ));
}

#[tokio::test]
async fn an_unchanged_fetch_clears_a_published_in_flight_summary() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
    let lease = h.provider_service.acquire(id).await.unwrap();
    gate.open();
    h.fetch_directly(id).await;
    gate.arrived().await;
    gate.close();
    let fetch = h.spawn_fetch(id).await;
    gate.arrived().await;

    h.provider_service
        .publish_summary(id, lease.generation)
        .await;
    gate.open();
    fetch.await.unwrap();

    let last = h
        .bus
        .events()
        .into_iter()
        .rev()
        .find_map(|event| match event {
            CoreEvent::ProviderStatusUpdated { provider } => Some(provider),
            _ => None,
        })
        .unwrap();
    assert!(!last.health.in_flight);
}

#[tokio::test]
async fn a_failed_fetch_keeps_the_time_of_the_last_successful_one() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    h.provider_service.acquire(id).await.unwrap();
    let fetch_status = || async { h.repository.get_provider_fetch_status(id).await.unwrap() };

    h.fetch_directly(id).await;
    let first = fetch_status().await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    h.fetch_directly(id).await;
    let second = fetch_status().await;
    assert!(second.fetched_at > first.fetched_at);
    assert_eq!(second.succeeded_at, second.fetched_at);

    *stub.pipelines.lock().unwrap() = Err(DomainError::NetworkError("offline".into()));
    tokio::time::sleep(Duration::from_millis(5)).await;
    h.fetch_directly(id).await;
    let failed = fetch_status().await;
    assert!(failed.fetched_at > second.fetched_at);
    assert_eq!(failed.succeeded_at, second.succeeded_at);

    let published = h
        .bus
        .events()
        .into_iter()
        .rev()
        .find_map(|event| match event {
            CoreEvent::ProviderStatusUpdated { provider } if provider.id == id => Some(provider),
            _ => None,
        })
        .unwrap();
    assert_eq!(published.last_fetch_at, failed.fetched_at);
    assert_eq!(published.last_success_at, second.succeeded_at);
}

#[tokio::test]
async fn a_provider_that_never_answers_times_out_after_thirty_seconds() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
    h.provider_service.acquire(id).await.unwrap();
    tokio::time::pause();
    let fetch = h.spawn_fetch(id).await;
    gate.arrived().await;
    tokio::time::advance(Duration::from_secs(31)).await;

    while !fetch.is_finished() {
        tokio::task::yield_now().await;
    }
    let result = fetch.await.unwrap();
    let status_read = h.repository.get_provider_fetch_status(id);
    tokio::pin!(status_read);
    let fetch = loop {
        if let std::task::Poll::Ready(read) = futures::poll!(status_read.as_mut()) {
            break read.unwrap();
        }
        tokio::task::yield_now().await;
    };

    let FetchResult::Committed(Err(DomainError::Timeout(message))) = result else {
        panic!("expected a timeout, got {result:?}");
    };
    assert_eq!(message, "Connection timeout - provider did not respond");
    assert_eq!(fetch.status, "error");
    assert!(fetch.error.unwrap().contains("did not respond"));
    assert_eq!(
        h.scheduler.view(id, Instant::now()).unwrap().last_failure,
        Some(crate::domain::FailureKind::Timeout)
    );
}

#[tokio::test]
async fn fetch_now_reports_unknown_providers_and_recent_attempts() {
    let h = Harness::new().await;
    h.start().await;
    let (id, _) = h.add_stub(StubProvider::new).await;

    let first = h
        .pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let again = h
        .pipeline_service
        .fetch_now(&[id, 4242], Duration::from_secs(5))
        .await;

    assert_eq!(first[0].state, RefreshState::Ok);
    assert_eq!(again[0].state, RefreshState::Recent);
    assert_eq!(again[1].state, RefreshState::NotFound);
}

async fn cache_with_rejected_deletes() -> Harness {
    let h = Harness::new().await;
    for pipeline_id in ["stub__1__web", "stub__1__api"] {
        h.repository
            .cache_run_history(pipeline_id, &[stub_run(pipeline_id, 1, 60)])
            .await
            .unwrap();
    }
    sqlx::query(
        "CREATE TRIGGER reject_run_history_delete BEFORE DELETE ON run_history_cache \
             BEGIN SELECT RAISE(ABORT, 'cache delete rejected'); END",
    )
    .execute(&h.pool)
    .await
    .unwrap();

    h
}

#[tokio::test]
async fn clearing_all_run_history_caches_reports_delete_errors() {
    let h = cache_with_rejected_deletes().await;

    let result = h.pipeline_service.clear_all_run_history_caches().await;

    assert!(matches!(
        result,
        Err(DomainError::DatabaseError(message)) if message.contains("cache delete rejected")
    ));
    assert_eq!(h.repository.get_run_history_cache_count().await.unwrap(), 2);

    sqlx::query("DROP TRIGGER reject_run_history_delete")
        .execute(&h.pool)
        .await
        .unwrap();
    h.pipeline_service
        .clear_all_run_history_caches()
        .await
        .unwrap();

    assert_eq!(h.repository.get_run_history_cache_count().await.unwrap(), 0);
}

async fn run_history_fixture(h: &Harness, gate: Option<&Arc<Gate>>) -> (i64, String) {
    let (id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            let stub = StubProvider::new(id).with_runs(vec![stub_run(&pipeline_id, 1, 60)]);
            match gate {
                Some(gate) => stub.run_gated(gate),
                None => stub,
            }
        })
        .await;
    h.fetch_directly(id).await;
    let pipeline_id = format!("stub__{id}__web");
    h.metrics
        .as_ref()
        .unwrap()
        .update_pipeline_config(&pipeline_id, true, 30)
        .await
        .unwrap();

    (id, pipeline_id)
}

async fn stored_metrics(h: &Harness, pipeline_id: &str) -> usize {
    h.metrics
        .as_ref()
        .unwrap()
        .query_metrics(crate::domain::MetricsQuery {
            pipeline_id: Some(pipeline_id.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .len()
}

#[tokio::test]
async fn fetched_run_history_is_cached_and_produces_metrics() {
    let h = Harness::with_metrics().await;
    let (_, pipeline_id) = run_history_fixture(&h, None).await;

    let page = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(1, 20))
        .await
        .unwrap();
    h.pipeline_service.settle().await;

    assert_eq!(page.runs.len(), 1);
    assert_eq!(
        h.repository
            .get_cached_run_count(&pipeline_id)
            .await
            .unwrap(),
        1
    );
    assert!(stored_metrics(&h, &pipeline_id).await > 0);
    assert_eq!(h.bus.count("metrics-generated"), 1);
    assert_eq!(h.bus.count("run-history-cache-invalidated"), 1);
}

#[tokio::test]
async fn cached_run_history_still_produces_metrics() {
    let h = Harness::with_metrics().await;
    let (_, pipeline_id) = run_history_fixture(&h, None).await;
    h.repository
        .cache_run_history(
            &pipeline_id,
            &[stub_run(&pipeline_id, 2, 60), stub_run(&pipeline_id, 1, 60)],
        )
        .await
        .unwrap();

    let page = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(1, 1))
        .await
        .unwrap();
    h.pipeline_service.settle().await;

    assert_eq!(page.runs.len(), 1);
    assert!(stored_metrics(&h, &pipeline_id).await > 0);
    assert_eq!(h.bus.count("metrics-generated"), 1);
}

#[tokio::test]
async fn a_run_history_fetch_finishing_after_a_removal_writes_nothing() {
    let h = Harness::with_metrics().await;
    let gate = Gate::new();
    let (id, pipeline_id) = run_history_fixture(&h, Some(&gate)).await;
    let fetch = tokio::spawn({
        let service = Arc::clone(&h.pipeline_service);
        let pipeline_id = pipeline_id.clone();
        async move {
            service
                .query_run_history(&pipeline_id, RunHistoryQuery::page(1, 20))
                .await
        }
    });
    gate.arrived().await;

    h.provider_service.remove_provider(id).await.unwrap();
    let removed_at = h.bus.events().len();
    gate.open();
    fetch.await.unwrap().unwrap();
    h.pipeline_service.settle().await;

    assert_eq!(
        h.repository
            .get_cached_run_count(&pipeline_id)
            .await
            .unwrap(),
        0
    );
    assert_eq!(stored_metrics(&h, &pipeline_id).await, 0);
    let later = &h.bus.names()[removed_at..];
    assert!(
        !later
            .iter()
            .any(|name| matches!(*name, "metrics-generated" | "run-history-cache-invalidated")),
        "{later:?}"
    );
}

#[tokio::test]
async fn cached_run_history_metrics_after_a_removal_write_nothing() {
    let h = Harness::with_metrics().await;
    let (id, pipeline_id) = run_history_fixture(&h, None).await;
    h.repository
        .cache_run_history(
            &pipeline_id,
            &[stub_run(&pipeline_id, 2, 60), stub_run(&pipeline_id, 1, 60)],
        )
        .await
        .unwrap();
    let lock = h.provider_service.commit_lock(id);
    let held = lock.lock().await;
    let removal = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.remove_provider(id).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    let page = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(1, 1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(held);
    removal.await.unwrap().unwrap();
    h.pipeline_service.settle().await;

    assert_eq!(page.runs.len(), 1);
    assert_eq!(stored_metrics(&h, &pipeline_id).await, 0);
    assert_eq!(h.bus.count("metrics-generated"), 0);
    assert_eq!(
        h.repository
            .get_cached_run_count(&pipeline_id)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn first_page_of_a_cold_history_says_more_runs_follow() {
    let h = Harness::new().await;
    let (id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            let runs = (1..=35).map(|n| stub_run(&pipeline_id, n, 60)).collect();
            StubProvider::new(id).with_runs(runs)
        })
        .await;
    h.fetch_directly(id).await;
    let pipeline_id = format!("stub__{id}__web");

    let first = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(1, 20))
        .await
        .unwrap();
    h.pipeline_service.settle().await;
    let last = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(2, 20))
        .await
        .unwrap();
    h.pipeline_service.settle().await;

    assert_eq!(first.runs.len(), 20);
    assert_eq!(first.total_count, 35);
    assert!(first.has_more, "page 1 has_more={}", first.has_more);
    assert_eq!(last.runs.len(), 15);
    assert!(!last.has_more, "page 2 has_more={}", last.has_more);
}

#[tokio::test]
async fn history_pagination_passes_a_thousand_runs_and_stops_on_the_last_page() {
    let h = Harness::new().await;
    let started = Utc::now() - chrono::Duration::minutes(1100);
    let (id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            let runs = (1..=1100)
                .rev()
                .map(|number| {
                    let mut run = stub_run(&pipeline_id, number, 60);
                    run.started_at = started + chrono::Duration::minutes(number);
                    run
                })
                .collect();
            StubProvider::new(id).with_runs(runs)
        })
        .await;
    h.fetch_directly(id).await;
    let pipeline_id = format!("stub__{id}__web");

    for (page, newest, oldest, has_more) in [
        (1, 1100, 1081, true),
        (50, 120, 101, true),
        (51, 100, 81, true),
        (55, 20, 1, false),
    ] {
        let history = h
            .pipeline_service
            .query_run_history(&pipeline_id, RunHistoryQuery::page(page, 20))
            .await
            .unwrap();
        h.pipeline_service.settle().await;
        let numbers: Vec<_> = history.runs.iter().map(|run| run.run_number).collect();
        assert_eq!(
            numbers,
            (oldest..=newest).rev().collect::<Vec<_>>(),
            "page {page}"
        );
        assert_eq!(history.has_more, has_more, "page {page}");
        if !has_more {
            assert!(history.is_complete);
            assert_eq!(history.total_count, 1100);
            assert_eq!(history.total_pages, 55);
        }
    }

    let past_end = h
        .pipeline_service
        .query_run_history(&pipeline_id, RunHistoryQuery::page(56, 20))
        .await
        .unwrap();
    h.pipeline_service.settle().await;
    assert!(past_end.runs.is_empty());
    assert!(!past_end.has_more);
    assert!(past_end.is_complete);
    assert_eq!(past_end.total_count, 1100);
}

fn archivable_run(
    pipeline_id: &str, number: i64, status: PipelineStatus, archivable: bool,
) -> PipelineRun {
    let mut run = stub_run(pipeline_id, number, 60);
    run.status = status;
    run.metadata
        .insert("archivable".to_string(), json!(archivable));
    run
}

async fn cached_numbers(h: &Harness, pipeline_id: &str) -> Vec<i64> {
    h.repository
        .get_cached_run_history(pipeline_id, 100, RunHistoryOrder::RunNumber)
        .await
        .unwrap()
        .iter()
        .map(|run| run.run_number)
        .collect()
}

#[tokio::test]
async fn the_default_history_policy_keeps_unbounded_cache_reconciliation() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    let target = pipeline(id, "web", PipelineStatus::Success);
    h.repository
        .cache_run_history(
            &target.id,
            &[
                archivable_run(&target.id, 1, PipelineStatus::Success, true),
                archivable_run(&target.id, 7, PipelineStatus::Success, true),
            ],
        )
        .await
        .unwrap();
    let api = [stub_run(&target.id, 5, 60)];

    h.pipeline_service
        .merge_run_cache(stub.as_ref(), &target, &api)
        .await
        .unwrap();

    assert_eq!(cached_numbers(&h, &target.id).await, [5, 1]);
    let kept = h
        .repository
        .get_cached_run(&target.id, 1)
        .await
        .unwrap()
        .unwrap();
    assert!(!kept.metadata.contains_key("archived"));
}

#[tokio::test]
async fn a_bounded_policy_retains_eligible_terminal_runs_and_deletes_the_rest() {
    let h = Harness::new().await;
    let (id, stub) = h
        .add_stub(|id| StubProvider::new(id).bounded_history())
        .await;
    let mut target = pipeline(id, "web", PipelineStatus::Success);
    target.id = format!("{STUB_BOUNDED}__{id}__web");
    target.provider_type = STUB_BOUNDED.to_string();
    h.repository
        .cache_run_history(
            &target.id,
            &[
                archivable_run(&target.id, 1, PipelineStatus::Success, true),
                archivable_run(&target.id, 2, PipelineStatus::Success, false),
                archivable_run(&target.id, 3, PipelineStatus::Running, true),
            ],
        )
        .await
        .unwrap();
    let api = [stub_run(&target.id, 10, 60)];

    h.pipeline_service
        .merge_run_cache(stub.as_ref(), &target, &api)
        .await
        .unwrap();

    assert_eq!(cached_numbers(&h, &target.id).await, [10, 1]);
    let kept = h
        .repository
        .get_cached_run(&target.id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.metadata["archived"], json!(true));
}

#[tokio::test]
async fn missing_remote_details_keep_the_original_error_unless_archive_eligible() {
    let h = Harness::new().await;
    for (bounded, archivable, archived) in [
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        let (id, stub) = h
            .add_stub(|id| {
                let stub = StubProvider::new(id);
                if bounded {
                    stub.bounded_history()
                } else {
                    stub
                }
            })
            .await;
        let mut target = pipeline(id, "web", PipelineStatus::Success);
        target.id = format!("{STUB_BOUNDED}__{id}__web");
        h.repository
            .cache_run_history(
                &target.id,
                &[archivable_run(
                    &target.id,
                    4,
                    PipelineStatus::Success,
                    archivable,
                )],
            )
            .await
            .unwrap();
        *stub.run_response.lock().unwrap() =
            Some(Err(DomainError::PipelineNotFound("gone".to_string())));

        let result = h
            .pipeline_service
            .fetch_run_details_from(stub.as_ref(), &target, 4)
            .await;

        match result {
            Ok(run) => {
                assert!(archived);
                assert_eq!(run.metadata["archived"], json!(true));
            }
            Err(error) => {
                assert!(!archived);
                assert!(
                    matches!(error, DomainError::PipelineNotFound(message) if message == "gone")
                );
            }
        }
    }
}

#[tokio::test]
async fn headless_overdue_checks_honour_the_plugin_policy() {
    let h = Harness::new().await;
    let repository = &h.repository;
    let mut ids = Vec::new();
    for provider_type in [STUB, STUB_BOUNDED] {
        let mut config = h.config(provider_type, 30);
        config.provider_type = provider_type.to_string();
        let provider_id = repository.add_provider(&config).await.unwrap();
        let mut target = pipeline(provider_id, "build", PipelineStatus::Running);
        target.id = format!("{provider_type}__{provider_id}__build");
        target.provider_type = provider_type.to_string();
        repository
            .update_pipelines_cache(provider_id, std::slice::from_ref(&target))
            .await
            .unwrap();
        repository
            .cache_run_history(&target.id, &ten_minute_history(45))
            .await
            .unwrap();
        ids.push(provider_id);
    }

    assert!(h.pipeline_service.update_overdue_pipelines(ids[0]).await);
    assert!(!h.pipeline_service.update_overdue_pipelines(ids[1]).await);

    let cached = h.pipeline_service.get_cached_pipelines(None).await.unwrap();
    let typical = |provider_id| {
        cached
            .iter()
            .find(|p| p.provider_id == provider_id)
            .unwrap()
            .typical_duration_seconds
    };
    assert_eq!(typical(ids[0]), Some(600));
    assert_eq!(typical(ids[1]), None);
}
