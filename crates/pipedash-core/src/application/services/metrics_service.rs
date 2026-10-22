use std::sync::Arc;

use chrono::Utc;

use crate::domain::{
    AggregatedMetrics,
    DomainError,
    DomainResult,
    GlobalMetricsConfig,
    MetricEntry,
    MetricMetadata,
    MetricType,
    MetricsConfig,
    MetricsQuery,
    MetricsStats,
    PipelineRun,
    PipelineStatus,
};
use crate::infrastructure::database::{
    MetricsRepository,
    RecordMetrics,
};
use crate::infrastructure::deduplication::hash_pipeline_run;

pub struct MetricsService {
    repository: Arc<MetricsRepository>,
}

impl MetricsService {
    pub fn new(repository: Arc<MetricsRepository>) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> Arc<MetricsRepository> {
        Arc::clone(&self.repository)
    }

    pub async fn get_global_config(&self) -> DomainResult<GlobalMetricsConfig> {
        self.repository.get_global_config().await
    }

    pub async fn update_global_config(
        &self, enabled: bool, default_retention_days: i64,
    ) -> DomainResult<()> {
        if default_retention_days < 1 {
            return Err(DomainError::InvalidConfig(
                "Retention days must be at least 1".to_string(),
            ));
        }

        let previous_config = self.repository.get_global_config().await?;

        self.repository
            .update_global_config(enabled, default_retention_days)
            .await?;

        if enabled && !previous_config.enabled {
            tracing::info!("Global metrics re-enabled - resetting all processing states");
            self.repository.reset_all_processing_states().await?;
        }

        Ok(())
    }

    pub async fn get_pipeline_config(
        &self, pipeline_id: &str,
    ) -> DomainResult<Option<MetricsConfig>> {
        self.repository.get_pipeline_config(pipeline_id).await
    }

    pub async fn get_effective_pipeline_config(
        &self, pipeline_id: &str,
    ) -> DomainResult<MetricsConfig> {
        if let Some(config) = self.repository.get_pipeline_config(pipeline_id).await? {
            Ok(config)
        } else {
            let global_config = self.repository.get_global_config().await?;
            Ok(MetricsConfig {
                pipeline_id: pipeline_id.to_string(),
                enabled: global_config.enabled,
                retention_days: global_config.default_retention_days,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
        }
    }

    pub async fn update_pipeline_config(
        &self, pipeline_id: &str, enabled: bool, retention_days: i64,
    ) -> DomainResult<()> {
        if retention_days < 1 {
            return Err(DomainError::InvalidConfig(
                "Retention days must be at least 1".to_string(),
            ));
        }

        let previous_config = self.get_effective_pipeline_config(pipeline_id).await?;

        self.repository
            .upsert_pipeline_config(pipeline_id, enabled, retention_days)
            .await?;

        if enabled && !previous_config.enabled {
            tracing::info!(
                pipeline_id = %pipeline_id,
                "Metrics re-enabled for pipeline - resetting processing state"
            );
            self.repository.reset_processing_state(pipeline_id).await?;
        }

        Ok(())
    }

    pub async fn extract_and_store_metrics(
        &self, pipeline_id: &str, runs: &[PipelineRun],
    ) -> DomainResult<usize> {
        if runs.is_empty() {
            return Ok(0);
        }

        let config = self.get_effective_pipeline_config(pipeline_id).await?;
        if !config.enabled {
            return Ok(0);
        }
        let retention_cutoff = chrono::Duration::try_days(config.retention_days)
            .and_then(|retention| Utc::now().checked_sub_signed(retention))
            .ok_or_else(|| {
                DomainError::InvalidConfig(
                    "Metrics retention is outside the supported date range".to_string(),
                )
            })?;

        let mut records = Vec::new();
        let mut has_regular_runs = false;
        for run in runs.iter().filter(|run| !is_snapshot(run)) {
            if is_native_history(run) {
                if is_final(&run.status) && run.started_at >= retention_cutoff {
                    records.push(record_metrics(pipeline_id, run));
                }
            } else {
                has_regular_runs = true;
            }
        }
        let refreshed = self
            .repository
            .refresh_record_metrics(pipeline_id, &records)
            .await?;

        let stored = if has_regular_runs {
            self.store_new_runs(pipeline_id, runs).await?
        } else {
            0
        };

        Ok(refreshed + stored)
    }

    async fn store_new_runs(&self, pipeline_id: &str, runs: &[PipelineRun]) -> DomainResult<usize> {
        if runs.is_empty() {
            return Ok(0);
        }

        let last_processed = self
            .repository
            .get_last_processed_run(pipeline_id)
            .await?
            .unwrap_or(0);

        tracing::debug!(
            pipeline_id = %pipeline_id,
            total_runs = runs.len(),
            last_processed = last_processed,
            "Checking runs for metrics extraction"
        );

        let new_runs: Vec<_> = runs
            .iter()
            .filter(|run| {
                !is_snapshot(run)
                    && !is_native_history(run)
                    && run.run_number > last_processed
                    && is_final(&run.status)
            })
            .collect();

        tracing::debug!(
            pipeline_id = %pipeline_id,
            new_runs_count = new_runs.len(),
            filtered_out = runs.len() - new_runs.len(),
            "Filtered runs for processing"
        );

        if new_runs.is_empty() && !runs.is_empty() {
            tracing::warn!(
                pipeline_id = %pipeline_id,
                total_runs = runs.len(),
                last_processed_run = last_processed,
                "No new runs to process - possible processing state corruption"
            );
        }

        if new_runs.is_empty() {
            return Ok(0);
        }

        const MAX_BATCH_SIZE: usize = 200;
        let runs_to_process = if new_runs.len() > MAX_BATCH_SIZE {
            tracing::debug!(
                available = new_runs.len(),
                processing = MAX_BATCH_SIZE,
                pipeline_id = %pipeline_id,
                "Limiting batch size"
            );
            &new_runs[..MAX_BATCH_SIZE]
        } else {
            tracing::debug!(
                count = new_runs.len(),
                pipeline_id = %pipeline_id,
                last_processed = last_processed,
                "Processing new runs"
            );
            &new_runs[..]
        };

        let mut metrics = Vec::new();
        let mut max_run_number = last_processed;

        for run in runs_to_process {
            max_run_number = max_run_number.max(run.run_number);
            metrics.extend(
                record_metrics(pipeline_id, run)
                    .entries
                    .into_iter()
                    .flatten(),
            );
        }

        if metrics.is_empty() {
            return Ok(0);
        }

        let start = std::time::Instant::now();
        let inserted = self.repository.insert_metrics_batch(metrics).await?;
        let insert_duration = start.elapsed();

        if insert_duration.as_secs() > 2 {
            tracing::warn!(
                duration_ms = insert_duration.as_millis(),
                count = inserted,
                "Slow metrics insert"
            );
        }

        self.repository
            .update_last_processed_run(pipeline_id, max_run_number)
            .await?;

        tracing::debug!(
            inserted = inserted,
            pipeline_id = %pipeline_id,
            duration_ms = insert_duration.as_millis(),
            last_run = max_run_number,
            "Stored metrics"
        );

        Ok(inserted)
    }

    pub async fn query_metrics(&self, query: MetricsQuery) -> DomainResult<Vec<MetricEntry>> {
        self.repository.query_metrics(query).await
    }

    pub async fn query_aggregated_metrics(
        &self, query: MetricsQuery,
    ) -> DomainResult<AggregatedMetrics> {
        if let Some(pipeline_id) = &query.pipeline_id {
            let config = self.get_effective_pipeline_config(pipeline_id).await?;
            if config.enabled {
                let existing_count = self
                    .repository
                    .count_metrics_for_pipeline(pipeline_id)
                    .await
                    .unwrap_or(0);

                if existing_count == 0 {
                    if let Ok(true) = self
                        .repository
                        .check_processing_state_corruption(pipeline_id)
                        .await
                    {
                        tracing::warn!(
                            pipeline_id = %pipeline_id,
                            "Detected corruption during metrics query - auto-repairing"
                        );
                        let _ = self.repository.reset_processing_state(pipeline_id).await;
                    }
                }
            }
        }

        self.repository.query_aggregated_metrics(query).await
    }

    pub async fn cleanup_old_metrics(&self) -> DomainResult<usize> {
        self.repository.delete_old_metrics(None).await
    }

    pub async fn get_storage_stats(&self) -> DomainResult<MetricsStats> {
        self.repository.get_storage_stats().await
    }

    pub async fn flush_metrics(
        &self, pipeline_id: Option<&str>, skip_vacuum: bool,
    ) -> DomainResult<usize> {
        let result = self
            .repository
            .flush_metrics(pipeline_id, skip_vacuum)
            .await?;

        if let Some(pipeline) = pipeline_id {
            self.repository.reset_processing_state(pipeline).await?;
        } else {
            self.repository.reset_all_processing_states().await?;
        }

        Ok(result)
    }

    pub async fn check_and_repair_corruption(&self) -> DomainResult<Vec<String>> {
        let repaired = self.repository.reset_all_corrupted_states().await?;

        if !repaired.is_empty() {
            tracing::warn!(
                count = repaired.len(),
                pipelines = ?repaired,
                "Detected and repaired corrupted metrics processing states"
            );
        }

        Ok(repaired)
    }

    pub async fn reset_pipeline_processing(&self, pipeline_id: &str) -> DomainResult<()> {
        tracing::info!(pipeline_id = %pipeline_id, "Resetting metrics processing state");
        self.repository.reset_processing_state(pipeline_id).await?;
        Ok(())
    }
}

fn is_snapshot(run: &PipelineRun) -> bool {
    run.metadata
        .get("snapshot")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

fn metadata_text<'a>(run: &'a PipelineRun, key: &str) -> Option<&'a str> {
    run.metadata.get(key).and_then(serde_json::Value::as_str)
}

fn is_native_history(run: &PipelineRun) -> bool {
    metadata_text(run, "record_source") == Some("status.history")
        && metadata_text(run, "history_scope") == Some("bounded_reconciliation")
}

fn is_final(status: &PipelineStatus) -> bool {
    matches!(
        status,
        PipelineStatus::Success
            | PipelineStatus::Failed
            | PipelineStatus::Cancelled
            | PipelineStatus::Skipped
    )
}

fn record_metrics(pipeline_id: &str, run: &PipelineRun) -> RecordMetrics {
    let status_str = run.status.as_str();

    let run_hash = hash_pipeline_run(
        run.run_number,
        status_str,
        run.branch.as_deref(),
        &run.started_at.to_rfc3339(),
        run.duration_seconds,
        run.commit_sha.as_deref(),
    );

    let metadata = MetricMetadata {
        status: Some(status_str.to_string()),
        branch: run.branch.clone(),
        repository: None,
        actor: run.actor.clone(),
    }
    .to_json();

    let entry = |metric_type, value| MetricEntry {
        id: 0,
        pipeline_id: pipeline_id.to_string(),
        run_number: run.run_number,
        timestamp: run.started_at,
        metric_type,
        value,
        metadata: Some(metadata.clone()),
        created_at: Utc::now(),
        run_hash: Some(run_hash.clone()),
    };

    let success_value = match run.status {
        PipelineStatus::Success => Some(100.0),
        PipelineStatus::Failed | PipelineStatus::Cancelled => Some(0.0),
        _ => None,
    };

    let entries = [
        run.duration_seconds
            .map(|duration| entry(MetricType::RunDuration, duration as f64)),
        success_value.map(|value| entry(MetricType::SuccessRate, value)),
        success_value.map(|_| entry(MetricType::RunFrequency, 1.0)),
    ];

    RecordMetrics {
        run_number: run.run_number,
        run_hash,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn state_snapshots_do_not_count_as_failed_reconciliations() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::infrastructure::database::init_database(dir.path().join("metrics.db"))
            .await
            .unwrap();
        let service = MetricsService::new(Arc::new(MetricsRepository::new(pool.clone())));
        service.update_global_config(true, 30).await.unwrap();
        let mut run: PipelineRun = serde_json::from_value(json!({
            "id": "snapshot", "pipeline_id": "fluxcd__1__Kustomization__apps__web", "run_number": 1,
            "status": "failed", "started_at": "2026-10-21T00:00:00Z", "logs_url": "",
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
        let success: f64 = sqlx::query_scalar(
            "SELECT value FROM pipeline_metrics WHERE metric_type = 'success_rate'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(success, 100.0);
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

    fn find(
        entries: &[MetricEntry], run_number: i64, metric_type: MetricType,
    ) -> Option<&MetricEntry> {
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
        let started = chrono::DateTime::from_timestamp_millis(Utc::now().timestamp_millis())
            .unwrap()
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

    async fn assert_concurrent_refresh_is_deduplicated(
        service: &MetricsService, pipeline_id: &str,
    ) {
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

    async fn sqlite_service(dir: &tempfile::TempDir) -> MetricsService {
        let pool = crate::infrastructure::database::init_database(dir.path().join("metrics.db"))
            .await
            .unwrap();
        MetricsService::new(Arc::new(MetricsRepository::new(pool)))
    }

    #[tokio::test]
    async fn native_history_refreshes_changed_records_without_inflating_counts() {
        let dir = tempfile::tempdir().unwrap();
        let service = sqlite_service(&dir).await;

        assert_native_history_refresh(&service, "fluxcd__1__Kustomization__apps__web").await;
    }

    #[tokio::test]
    async fn concurrent_native_refreshes_store_each_metric_once() {
        let dir = tempfile::tempdir().unwrap();
        let service = sqlite_service(&dir).await;

        assert_concurrent_refresh_is_deduplicated(&service, "fluxcd__1__Kustomization__apps__api")
            .await;
    }

    #[tokio::test]
    async fn runs_without_native_provenance_keep_their_first_observation() {
        let dir = tempfile::tempdir().unwrap();
        let service = sqlite_service(&dir).await;
        let pipeline_id = "github__1__acme__repo";
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
        let service = MetricsService::new(Arc::new(MetricsRepository::new_from_pool(
            DatabasePool::Postgres(pool.clone()),
        )));
        let suffix = Utc::now().timestamp_nanos_opt().unwrap();
        let refresh_id = format!("fluxcd__postgres__Kustomization__refresh__{suffix}");
        let concurrent_id = format!("fluxcd__postgres__Kustomization__concurrent__{suffix}");
        let retention_id = format!("fluxcd__postgres__HelmRelease__retention__{suffix}");

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
        assert_native_retention(&service, "fluxcd__1__HelmRelease__apps__retention").await;
    }
}
