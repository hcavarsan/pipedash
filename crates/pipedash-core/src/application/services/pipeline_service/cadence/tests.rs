use std::collections::HashMap;

use chrono::{
    DateTime,
    Duration,
    Utc,
};
use serde_json::json;

use super::{
    is_quiet,
    usual_gap,
};
use crate::application::services::pipeline_service::PipelineService;
use crate::application::test_support::{
    Gate,
    Harness,
    StubProvider,
};
use crate::domain::{
    Pipeline,
    PipelineCadence,
    PipelineRun,
    ProviderConfig,
};

fn daily_runs(pipeline_id: &str, last: DateTime<Utc>, count: i64) -> Vec<PipelineRun> {
    spaced_runs(pipeline_id, last, count, 24)
}

fn spaced_runs(
    pipeline_id: &str, last: DateTime<Utc>, count: i64, hours_apart: i64,
) -> Vec<PipelineRun> {
    (0..count)
        .map(|step| {
            serde_json::from_value(json!({
                "id": format!("{pipeline_id}__{step}"), "pipeline_id": pipeline_id,
                "run_number": count - step, "status": "success",
                "started_at": last - Duration::hours(hours_apart * step), "logs_url": ""
            }))
            .unwrap()
        })
        .collect()
}

async fn learn(service: &PipelineService, provider_id: i64) -> bool {
    let staged = service.stage_provider_gaps(provider_id).await.unwrap();
    service.apply_gaps(staged)
}

fn nightly(provider_id: i64, last_run: DateTime<Utc>) -> Pipeline {
    serde_json::from_value(json!({
        "id": format!("stub__{provider_id}__nightly"), "provider_id": provider_id,
        "provider_type": "stub", "name": "nightly", "status": "success",
        "repository": "acme/app", "last_run": last_run, "last_updated": Utc::now()
    }))
    .unwrap()
}

async fn harness_with(last_run: DateTime<Utc>, runs: i64) -> (Harness, i64) {
    let h = Harness::new().await;
    let provider_id = h
        .repository
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
    let id = format!("stub__{provider_id}__nightly");
    let pipeline: Pipeline = serde_json::from_value(json!({
        "id": id, "provider_id": provider_id, "provider_type": "stub",
        "name": "nightly", "status": "success", "repository": "acme/app",
        "last_run": last_run, "last_updated": Utc::now()
    }))
    .unwrap();
    h.repository
        .update_pipelines_cache(provider_id, &[pipeline])
        .await
        .unwrap();
    h.repository
        .cache_run_history(&id, &daily_runs(&id, last_run, runs))
        .await
        .unwrap();

    (h, provider_id)
}

#[test]
fn usual_gap_is_the_median_between_run_starts() {
    let now = Utc::now();
    let mut runs = daily_runs("p", now, 5);
    runs[4].started_at = now - Duration::days(30);

    assert_eq!(usual_gap(&runs), Some(86_400));
    assert_eq!(usual_gap(&daily_runs("p", now, 3)), None);
}

#[test]
fn a_pipeline_goes_quiet_after_twice_its_usual_gap() {
    let now = Utc::now();

    assert!(!is_quiet(Some(now - Duration::hours(47)), 86_400, now));
    assert!(is_quiet(Some(now - Duration::hours(49)), 86_400, now));
    assert!(!is_quiet(None, 86_400, now));
}

#[tokio::test]
async fn a_nightly_pipeline_that_stopped_firing_is_marked_quiet_once() {
    let (h, provider_id) = harness_with(Utc::now() - Duration::days(3), 6).await;
    let service = &h.pipeline_service;

    let changed = learn(service, provider_id).await;
    let unchanged = learn(service, provider_id).await;
    let pipelines = service.get_cached_pipelines(None).await.unwrap();

    assert!(changed);
    assert!(!unchanged);
    assert_eq!(
        pipelines[0].cadence,
        Some(PipelineCadence {
            usual_gap_seconds: 86_400,
            quiet: true,
        })
    );
}

#[tokio::test]
async fn a_pipeline_on_schedule_or_with_too_few_runs_is_not_quiet() {
    let (on_time, on_time_provider) = harness_with(Utc::now() - Duration::hours(12), 6).await;
    let (sparse, sparse_provider) = harness_with(Utc::now() - Duration::days(3), 3).await;

    assert!(!learn(&on_time.pipeline_service, on_time_provider).await);
    assert!(!learn(&sparse.pipeline_service, sparse_provider).await);
    let on_time_pipelines = on_time
        .pipeline_service
        .get_cached_pipelines(None)
        .await
        .unwrap();
    let sparse_pipelines = sparse
        .pipeline_service
        .get_cached_pipelines(None)
        .await
        .unwrap();

    assert_eq!(
        on_time_pipelines[0].cadence.map(|cadence| cadence.quiet),
        Some(false)
    );
    assert_eq!(sparse_pipelines[0].cadence, None);
}

#[tokio::test]
async fn a_pipeline_seen_with_too_few_runs_learns_its_gap_after_the_next_run() {
    let h = Harness::new().await;
    let first_last = Utc::now() - Duration::days(4);
    let (provider_id, stub) = h.add_stub(StubProvider::new).await;
    let id = format!("stub__{provider_id}__nightly");
    *stub.runs.lock().unwrap() = daily_runs(&id, first_last, 3);
    h.repository
        .update_pipelines_cache(provider_id, &[nightly(provider_id, first_last)])
        .await
        .unwrap();
    let service = &h.pipeline_service;
    learn(service, provider_id).await;
    let unlearned = service.get_cached_pipelines(None).await.unwrap();

    let next_last = first_last + Duration::days(1);
    *stub.runs.lock().unwrap() = daily_runs(&id, next_last, 4);
    h.repository.clear_cached_run_history(&id).await.unwrap();
    h.repository
        .update_pipelines_cache(provider_id, &[nightly(provider_id, next_last)])
        .await
        .unwrap();
    let changed = learn(service, provider_id).await;
    let learned = service.get_cached_pipelines(None).await.unwrap();

    assert_eq!(unlearned[0].cadence, None);
    assert!(changed);
    assert_eq!(
        learned[0].cadence,
        Some(PipelineCadence {
            usual_gap_seconds: 86_400,
            quiet: true,
        })
    );
}

#[tokio::test]
async fn pipelines_whose_history_keeps_failing_do_not_starve_the_rest() {
    let h = Harness::new().await;
    let last_run = Utc::now() - Duration::days(3);
    let (provider_id, stub) = h.add_stub(StubProvider::new).await;
    let ids: Vec<String> = (0..5)
        .map(|n| format!("stub__{provider_id}__job{n}"))
        .collect();
    let pipelines: Vec<Pipeline> = ids
        .iter()
        .enumerate()
        .map(|(n, id)| {
            serde_json::from_value(json!({
                "id": id, "provider_id": provider_id, "provider_type": "stub",
                "name": id, "status": "success", "repository": "acme/app",
                "last_run": last_run, "last_updated": Utc::now() - Duration::minutes(n as i64)
            }))
            .unwrap()
        })
        .collect();
    *stub.runs.lock().unwrap() = daily_runs(&ids[4], last_run, 6);
    *stub.failing_histories.lock().unwrap() = ids[..4].to_vec();
    h.repository
        .update_pipelines_cache(provider_id, &pipelines)
        .await
        .unwrap();
    let service = &h.pipeline_service;

    learn(service, provider_id).await;
    learn(service, provider_id).await;
    let cached = service.get_cached_pipelines(None).await.unwrap();
    let healthy = cached.iter().find(|p| p.id == ids[4]).unwrap();

    assert_eq!(
        healthy.cadence,
        Some(PipelineCadence {
            usual_gap_seconds: 86_400,
            quiet: true,
        })
    );
}

#[tokio::test]
async fn failing_relearns_of_known_gaps_do_not_starve_the_rest() {
    let h = Harness::new().await;
    let hourly_last = Utc::now() - Duration::hours(13);
    let (provider_id, stub) = h.add_stub(StubProvider::new).await;
    let ids: Vec<String> = (0..5)
        .map(|n| format!("stub__{provider_id}__job{n}"))
        .collect();
    let pipelines_at = |last_run: DateTime<Utc>| -> Vec<Pipeline> {
        ids.iter()
            .enumerate()
            .map(|(n, id)| {
                serde_json::from_value(json!({
                    "id": id, "provider_id": provider_id, "provider_type": "stub",
                    "name": id, "status": "success", "repository": "acme/app",
                    "last_run": last_run, "last_updated": Utc::now() - Duration::minutes(n as i64)
                }))
                .unwrap()
            })
            .collect()
    };
    *stub.runs.lock().unwrap() = spaced_runs(&ids[4], hourly_last, 6, 1);
    h.repository
        .update_pipelines_cache(provider_id, &pipelines_at(hourly_last))
        .await
        .unwrap();
    let service = &h.pipeline_service;
    learn(service, provider_id).await;
    learn(service, provider_id).await;

    let daily_last = hourly_last + Duration::hours(1);
    *stub.runs.lock().unwrap() = daily_runs(&ids[4], daily_last, 6);
    *stub.failing_histories.lock().unwrap() = ids[..4].to_vec();
    for id in &ids {
        h.repository.clear_cached_run_history(id).await.unwrap();
    }
    h.repository
        .update_pipelines_cache(provider_id, &pipelines_at(daily_last))
        .await
        .unwrap();
    learn(service, provider_id).await;
    learn(service, provider_id).await;
    let cached = service.get_cached_pipelines(None).await.unwrap();
    let healthy = cached.iter().find(|p| p.id == ids[4]).unwrap();

    assert_eq!(
        healthy.cadence.map(|cadence| cadence.usual_gap_seconds),
        Some(86_400)
    );
}

#[tokio::test]
async fn a_cadence_fetch_dropped_by_an_invalidation_is_retried_on_the_next_pass() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let last_run = Utc::now() - Duration::days(3);
    let (provider_id, stub) = h
        .add_stub(|id| StubProvider::new(id).run_gated(&gate))
        .await;
    let id = format!("stub__{provider_id}__nightly");
    *stub.runs.lock().unwrap() = daily_runs(&id, last_run, 6);
    h.repository
        .update_pipelines_cache(provider_id, &[nightly(provider_id, last_run)])
        .await
        .unwrap();
    let service = &h.pipeline_service;

    let invalidate_mid_fetch = async {
        gate.arrived().await;
        service.invalidate_run_cache(&id).await;
        gate.open();
    };
    let (dropped, ()) = tokio::join!(learn(service, provider_id), invalidate_mid_fetch);
    let retried = learn(service, provider_id).await;
    let cached = service.get_cached_pipelines(None).await.unwrap();

    assert!(!dropped);
    assert!(retried);
    assert_eq!(
        cached[0].cadence,
        Some(PipelineCadence {
            usual_gap_seconds: 86_400,
            quiet: true,
        })
    );
}

#[tokio::test]
async fn a_pipeline_that_moves_to_a_slower_schedule_relearns_its_gap() {
    let h = Harness::new().await;
    let hourly_last = Utc::now() - Duration::hours(13);
    let (provider_id, stub) = h.add_stub(StubProvider::new).await;
    let id = format!("stub__{provider_id}__nightly");
    *stub.runs.lock().unwrap() = spaced_runs(&id, hourly_last, 6, 1);
    h.repository
        .update_pipelines_cache(provider_id, &[nightly(provider_id, hourly_last)])
        .await
        .unwrap();
    let service = &h.pipeline_service;
    learn(service, provider_id).await;
    let hourly = service.get_cached_pipelines(None).await.unwrap();

    let daily_last = hourly_last + Duration::hours(1);
    *stub.runs.lock().unwrap() = daily_runs(&id, daily_last, 6);
    h.repository.clear_cached_run_history(&id).await.unwrap();
    h.repository
        .update_pipelines_cache(provider_id, &[nightly(provider_id, daily_last)])
        .await
        .unwrap();
    learn(service, provider_id).await;
    let daily = service.get_cached_pipelines(None).await.unwrap();

    assert_eq!(
        hourly[0].cadence.map(|cadence| cadence.usual_gap_seconds),
        Some(3_600)
    );
    assert_eq!(
        daily[0].cadence,
        Some(PipelineCadence {
            usual_gap_seconds: 86_400,
            quiet: false,
        })
    );
}
