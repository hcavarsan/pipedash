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
use crate::application::test_support::Harness;
use crate::domain::{
    Pipeline,
    PipelineCadence,
    PipelineRun,
    ProviderConfig,
};

fn daily_runs(pipeline_id: &str, last: DateTime<Utc>, count: i64) -> Vec<PipelineRun> {
    (0..count)
        .map(|day| {
            serde_json::from_value(json!({
                "id": format!("{pipeline_id}__{day}"), "pipeline_id": pipeline_id,
                "run_number": count - day, "status": "success",
                "started_at": last - Duration::days(day), "logs_url": ""
            }))
            .unwrap()
        })
        .collect()
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

    let changed = service.update_provider_gaps(provider_id).await;
    let unchanged = service.update_provider_gaps(provider_id).await;
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

    assert!(
        !on_time
            .pipeline_service
            .update_provider_gaps(on_time_provider)
            .await
    );
    assert!(
        !sparse
            .pipeline_service
            .update_provider_gaps(sparse_provider)
            .await
    );
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
