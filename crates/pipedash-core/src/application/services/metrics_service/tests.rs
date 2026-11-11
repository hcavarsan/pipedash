use std::sync::Arc;

use chrono::Utc;
use serde_json::json;

use super::MetricsService;
use crate::application::test_support::RecordingBus;
use crate::domain::{
    MetricEntry,
    MetricType,
    MetricsQuery,
    PipelineRun,
    PipelineStatus,
};
use crate::event::{
    CoreEvent,
    NoOpEventBus,
};
use crate::infrastructure::database::{
    DatabasePool,
    MetricsRepository,
};

#[tokio::test]
async fn state_snapshots_do_not_count_as_failed_reconciliations() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::infrastructure::database::init_database(dir.path().join("metrics.db"))
        .await
        .unwrap();
    let service = MetricsService::new(
        Arc::new(MetricsRepository::new_from_pool(DatabasePool::Sqlite(
            pool.clone(),
        ))),
        Arc::new(NoOpEventBus),
    );
    service.update_global_config(true, 30).await.unwrap();
    let mut run: PipelineRun = serde_json::from_value(json!({
        "id": "snapshot", "pipeline_id": "stub__1__Kustomization__apps__web", "run_number": 1,
        "status": "failed", "started_at": Utc::now().to_rfc3339(), "logs_url": "",
        "metadata": { "snapshot": true }
    }))
    .unwrap();

    service
        .extract_and_store_metrics(&run.pipeline_id, std::slice::from_ref(&run))
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_metrics")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    run.metadata.clear();
    run.status = PipelineStatus::Success;
    service
        .extract_and_store_metrics(&run.pipeline_id, std::slice::from_ref(&run))
        .await
        .unwrap();
    let success: f64 =
        sqlx::query_scalar("SELECT value FROM pipeline_metrics WHERE metric_type = 'success_rate'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(success, 100.0);
}

#[tokio::test]
async fn runs_past_retention_are_not_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::infrastructure::database::init_database(dir.path().join("metrics.db"))
        .await
        .unwrap();
    let service = MetricsService::new(
        Arc::new(MetricsRepository::new_from_pool(DatabasePool::Sqlite(pool))),
        Arc::new(NoOpEventBus),
    );
    service.update_global_config(true, 7).await.unwrap();
    let pipeline_id = "stub__1__o__r__1";
    let run = |run_number: i64, days_ago: i64| -> PipelineRun {
        serde_json::from_value(json!({
            "id": format!("{pipeline_id}__{run_number}"), "pipeline_id": pipeline_id,
            "run_number": run_number, "status": "success",
            "started_at": (Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339(),
            "logs_url": ""
        }))
        .unwrap()
    };

    service
        .extract_and_store_metrics(pipeline_id, &[run(2, 1), run(1, 10)])
        .await
        .unwrap();

    let runs: Vec<_> = stored(&service, pipeline_id)
        .await
        .iter()
        .map(|entry| entry.run_number)
        .collect();
    assert_eq!(runs, [2, 2]);
}

#[tokio::test]
async fn run_that_finishes_after_a_newer_run_gets_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let service = sqlite_service(&dir).await;
    service.update_global_config(true, 30).await.unwrap();
    let pipeline_id = "stub__1__acme__app__deploy";
    let run = |run_number: i64, status: &str| -> PipelineRun {
        serde_json::from_value(json!({
            "id": format!("{pipeline_id}__{run_number}"), "pipeline_id": pipeline_id,
            "run_number": run_number, "status": status,
            "started_at": Utc::now().to_rfc3339(), "logs_url": "",
            "duration_seconds": 60
        }))
        .unwrap()
    };

    let first = service
        .extract_and_store_metrics(pipeline_id, &[run(11, "success"), run(10, "running")])
        .await
        .unwrap();
    assert_eq!(first, 3);
    let mut rerun_11 = run(11, "success");
    rerun_11.duration_seconds = Some(90);
    let second = service
        .extract_and_store_metrics(pipeline_id, &[rerun_11, run(10, "failed")])
        .await
        .unwrap();
    assert_eq!(second, 3);
    let repeated = service
        .extract_and_store_metrics(pipeline_id, &[run(11, "success"), run(10, "failed")])
        .await
        .unwrap();
    assert_eq!(repeated, 0);

    let entries = stored(&service, pipeline_id).await;
    assert_eq!(entries.len(), 6);
    assert_eq!(
        find(&entries, 10, MetricType::SuccessRate).unwrap().value,
        0.0
    );
    assert_eq!(
        find(&entries, 11, MetricType::SuccessRate).unwrap().value,
        100.0
    );
}

fn native_run(
    pipeline_id: &str, run_number: i64, started_at: &str, duration_seconds: Option<i64>,
) -> PipelineRun {
    let mut run: PipelineRun = serde_json::from_value(json!({
        "id": format!("{pipeline_id}__{run_number}"), "pipeline_id": pipeline_id,
        "run_number": run_number, "status": "success", "started_at": started_at,
        "logs_url": "",
        "metadata": {
            "record_source": "status.history", "history_scope": "bounded_reconciliation",
            "total_reconciliations": 1
        }
    }))
    .unwrap();
    run.duration_seconds = duration_seconds;
    run
}

fn snapshot_run(pipeline_id: &str, run_number: i64) -> PipelineRun {
    serde_json::from_value(json!({
        "id": format!("{pipeline_id}__{run_number}"), "pipeline_id": pipeline_id,
        "run_number": run_number, "status": "failed", "started_at": "2026-10-21T00:30:00Z",
        "logs_url": "",
        "metadata": {
            "record_source": "current_snapshot", "history_scope": "bounded_reconciliation",
            "snapshot": true
        }
    }))
    .unwrap()
}

async fn stored(service: &MetricsService, pipeline_id: &str) -> Vec<MetricEntry> {
    service
        .query_metrics(MetricsQuery {
            pipeline_id: Some(pipeline_id.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
}

fn find(entries: &[MetricEntry], run_number: i64, metric_type: MetricType) -> Option<&MetricEntry> {
    entries
        .iter()
        .find(|entry| entry.run_number == run_number && entry.metric_type == metric_type)
}

fn ids(entries: &[MetricEntry], run_number: i64) -> Vec<i64> {
    let mut ids: Vec<i64> = entries
        .iter()
        .filter(|entry| entry.run_number == run_number)
        .map(|entry| entry.id)
        .collect();
    ids.sort_unstable();
    ids
}

async fn assert_native_history_refresh(service: &MetricsService, pipeline_id: &str) {
    service
        .update_pipeline_config(pipeline_id, true, 30)
        .await
        .unwrap();
    let (older, newer) = (1_760_000_000_000_011, 1_760_000_600_000_022);
    let started = chrono::DateTime::from_timestamp_millis(Utc::now().timestamp_millis()).unwrap()
        - chrono::Duration::hours(1);
    let snapshot = snapshot_run(pipeline_id, newer + 1);
    let newest = native_run(
        pipeline_id,
        newer,
        &(started + chrono::Duration::minutes(10)).to_rfc3339(),
        Some(7),
    );
    let mut grouped = native_run(pipeline_id, older, &started.to_rfc3339(), Some(10));
    let batch = |grouped: &PipelineRun| vec![snapshot.clone(), newest.clone(), grouped.clone()];

    let first = service
        .extract_and_store_metrics(pipeline_id, &batch(&grouped))
        .await
        .unwrap();
    assert_eq!(first, 6);
    let before = stored(service, pipeline_id).await;
    assert_eq!(before.len(), 6);
    assert!(before.iter().all(|entry| entry.run_number != newer + 1));

    let repeated = service
        .extract_and_store_metrics(pipeline_id, &batch(&grouped))
        .await
        .unwrap();
    assert_eq!(repeated, 0);
    let unchanged = stored(service, pipeline_id).await;
    assert_eq!(ids(&unchanged, older), ids(&before, older));
    assert_eq!(ids(&unchanged, newer), ids(&before, newer));

    grouped.started_at = started + chrono::Duration::minutes(20);
    grouped.duration_seconds = Some(25);
    grouped
        .metadata
        .insert("total_reconciliations".to_string(), json!(9));
    service
        .extract_and_store_metrics(pipeline_id, &batch(&grouped))
        .await
        .unwrap();
    let refreshed = stored(service, pipeline_id).await;
    assert_eq!(refreshed.len(), 6);
    let duration = find(&refreshed, older, MetricType::RunDuration).unwrap();
    assert_eq!(duration.value, 25.0);
    assert_eq!(duration.timestamp, grouped.started_at);
    assert_eq!(ids(&refreshed, newer), ids(&before, newer));
    let frequency: f64 = refreshed
        .iter()
        .filter(|entry| entry.metric_type == MetricType::RunFrequency)
        .map(|entry| entry.value)
        .sum();
    assert_eq!(frequency, 2.0);

    grouped.started_at = started + chrono::Duration::minutes(21);
    grouped.duration_seconds = None;
    service
        .extract_and_store_metrics(pipeline_id, &batch(&grouped))
        .await
        .unwrap();
    let without_duration = stored(service, pipeline_id).await;
    assert_eq!(without_duration.len(), 5);
    assert!(find(&without_duration, older, MetricType::RunDuration).is_none());
    assert_eq!(
        find(&without_duration, older, MetricType::SuccessRate)
            .unwrap()
            .timestamp,
        grouped.started_at
    );

    let settled = service
        .extract_and_store_metrics(pipeline_id, &batch(&grouped))
        .await
        .unwrap();
    assert_eq!(settled, 0);
    assert_eq!(stored(service, pipeline_id).await.len(), 5);

    grouped.status = PipelineStatus::Skipped;
    let removed = service
        .extract_and_store_metrics(pipeline_id, &batch(&grouped))
        .await
        .unwrap();
    assert_eq!(removed, 2);
    assert!(stored(service, pipeline_id)
        .await
        .iter()
        .all(|entry| entry.run_number != older));
}

async fn assert_concurrent_refresh_is_deduplicated(service: &MetricsService, pipeline_id: &str) {
    service
        .update_pipeline_config(pipeline_id, true, 30)
        .await
        .unwrap();
    let runs = [native_run(
        pipeline_id,
        1_760_000_000_000_033,
        &Utc::now().to_rfc3339(),
        Some(5),
    )];

    let inserted: usize = futures::future::join_all(
        (0..8).map(|_| service.extract_and_store_metrics(pipeline_id, &runs)),
    )
    .await
    .into_iter()
    .map(Result::unwrap)
    .sum();

    assert_eq!(inserted, 3);
    assert_eq!(stored(service, pipeline_id).await.len(), 3);
}

pub(super) async fn sqlite_service(dir: &tempfile::TempDir) -> MetricsService {
    let pool = crate::infrastructure::database::init_database(dir.path().join("metrics.db"))
        .await
        .unwrap();
    MetricsService::new(
        Arc::new(MetricsRepository::new_from_pool(DatabasePool::Sqlite(pool))),
        Arc::new(NoOpEventBus),
    )
}

pub(super) fn metric_entry(
    pipeline_id: &str, run_number: i64, metric_type: MetricType, timestamp: &str, value: f64,
) -> MetricEntry {
    let timestamp = timestamp.parse::<chrono::DateTime<Utc>>().unwrap();
    MetricEntry {
        id: 0,
        pipeline_id: pipeline_id.to_string(),
        run_number,
        timestamp,
        metric_type,
        value,
        metadata: None,
        created_at: timestamp,
        run_hash: Some(format!(
            "{pipeline_id}-{run_number}-{}",
            metric_type.as_str()
        )),
    }
}

#[tokio::test]
async fn native_history_refreshes_changed_records_without_inflating_counts() {
    let dir = tempfile::tempdir().unwrap();
    let service = sqlite_service(&dir).await;

    assert_native_history_refresh(&service, "stub__1__Kustomization__apps__web").await;
}

#[tokio::test]
async fn concurrent_native_refreshes_store_each_metric_once() {
    let dir = tempfile::tempdir().unwrap();
    let service = sqlite_service(&dir).await;

    assert_concurrent_refresh_is_deduplicated(&service, "stub__1__Kustomization__apps__api").await;
}

#[tokio::test]
async fn runs_without_native_provenance_keep_their_first_observation() {
    let dir = tempfile::tempdir().unwrap();
    let service = sqlite_service(&dir).await;
    let pipeline_id = "stub__1__acme__repo";
    service
        .update_pipeline_config(pipeline_id, true, 30)
        .await
        .unwrap();
    let mut run = native_run(pipeline_id, 5, "2026-10-21T00:00:00Z", Some(10));
    run.metadata.clear();

    let first = service
        .extract_and_store_metrics(pipeline_id, std::slice::from_ref(&run))
        .await
        .unwrap();
    assert_eq!(first, 3);

    run.started_at = "2026-10-21T00:20:00Z".parse().unwrap();
    run.duration_seconds = Some(99);
    let second = service
        .extract_and_store_metrics(pipeline_id, std::slice::from_ref(&run))
        .await
        .unwrap();
    assert_eq!(second, 0);

    let entries = stored(&service, pipeline_id).await;
    assert_eq!(entries.len(), 3);
    assert_eq!(
        find(&entries, 5, MetricType::RunDuration).unwrap().value,
        10.0
    );
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn postgres_native_history_refreshes_changed_records_atomically() {
    use crate::infrastructure::database::DatabasePool;

    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
    let pool = crate::infrastructure::database::init_postgres_database(&url)
        .await
        .unwrap();
    let service = MetricsService::new(
        Arc::new(MetricsRepository::new_from_pool(DatabasePool::Postgres(
            pool.clone(),
        ))),
        Arc::new(NoOpEventBus),
    );
    let suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let refresh_id = format!("stub__postgres__Kustomization__refresh__{suffix}");
    let concurrent_id = format!("stub__postgres__Kustomization__concurrent__{suffix}");
    let retention_id = format!("stub__postgres__HelmRelease__retention__{suffix}");

    assert_native_history_refresh(&service, &refresh_id).await;
    assert_concurrent_refresh_is_deduplicated(&service, &concurrent_id).await;
    assert_native_retention(&service, &retention_id).await;

    for pipeline_id in [&refresh_id, &concurrent_id, &retention_id] {
        service
            .flush_metrics(Some(pipeline_id.as_str()), true)
            .await
            .unwrap();
        sqlx::query("DELETE FROM metrics_config WHERE pipeline_id = $1")
            .bind(pipeline_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
}

async fn assert_native_retention(service: &MetricsService, pipeline_id: &str) {
    let now = chrono::DateTime::from_timestamp_millis(Utc::now().timestamp_millis()).unwrap();
    service
        .update_pipeline_config(pipeline_id, true, 30)
        .await
        .unwrap();
    let mut expired = native_run(
        pipeline_id,
        1,
        &(now - chrono::Duration::days(2)).to_rfc3339(),
        Some(10),
    );
    let recent = native_run(
        pipeline_id,
        2,
        &(now - chrono::Duration::hours(1)).to_rfc3339(),
        Some(5),
    );
    service
        .extract_and_store_metrics(pipeline_id, &[expired.clone(), recent.clone()])
        .await
        .unwrap();
    assert_eq!(stored(service, pipeline_id).await.len(), 6);

    service
        .update_pipeline_config(pipeline_id, true, 1)
        .await
        .unwrap();
    assert_eq!(
        service
            .repository
            .delete_old_metrics(Some(pipeline_id))
            .await
            .unwrap(),
        3
    );
    let remaining = stored(service, pipeline_id).await;
    assert_eq!(remaining.len(), 3);
    assert!(remaining
        .iter()
        .all(|entry| entry.run_number == recent.run_number));

    let restored = service
        .extract_and_store_metrics(pipeline_id, &[expired.clone(), recent.clone()])
        .await
        .unwrap();
    assert_eq!(restored, 0);
    let after_read = stored(service, pipeline_id).await;
    assert_eq!(after_read.len(), 3);
    assert_eq!(
        ids(&after_read, recent.run_number),
        ids(&remaining, recent.run_number)
    );

    expired.started_at = now - chrono::Duration::minutes(1);
    expired.duration_seconds = Some(25);
    service
        .extract_and_store_metrics(pipeline_id, &[expired.clone(), recent])
        .await
        .unwrap();
    let renewed = stored(service, pipeline_id).await;
    assert_eq!(renewed.len(), 6);
    let metric = find(&renewed, expired.run_number, MetricType::RunDuration).unwrap();
    assert_eq!(metric.value, 25.0);
    assert_eq!(metric.timestamp, expired.started_at);
}

#[tokio::test]
async fn expired_native_history_does_not_restore_deleted_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let service = sqlite_service(&dir).await;
    assert_native_retention(&service, "stub__1__HelmRelease__apps__retention").await;
}

async fn recorded_service(dir: &tempfile::TempDir) -> (MetricsService, Arc<RecordingBus>) {
    let pool = crate::infrastructure::database::init_database(dir.path().join("metrics.db"))
        .await
        .unwrap();
    let bus = Arc::new(RecordingBus::default());
    let service = MetricsService::new(
        Arc::new(MetricsRepository::new_from_pool(DatabasePool::Sqlite(pool))),
        bus.clone(),
    );
    (service, bus)
}

#[tokio::test]
async fn saving_the_global_config_announces_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let (service, bus) = recorded_service(&dir).await;

    service.update_global_config(true, 14).await.unwrap();

    assert_eq!(bus.names(), ["metrics-global-config-changed"]);
}

#[tokio::test]
async fn saving_a_pipeline_config_announces_that_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let (service, bus) = recorded_service(&dir).await;

    service
        .update_pipeline_config("stub__1__web", true, 14)
        .await
        .unwrap();

    assert!(matches!(
        bus.events().as_slice(),
        [CoreEvent::MetricsConfigChanged { pipeline_id }] if pipeline_id == "stub__1__web"
    ));
}

#[tokio::test]
async fn a_rejected_config_save_announces_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (service, bus) = recorded_service(&dir).await;

    assert!(service.update_global_config(true, 0).await.is_err());
    assert!(service
        .update_pipeline_config("stub__1__web", true, 0)
        .await
        .is_err());

    assert!(bus.events().is_empty());
}
