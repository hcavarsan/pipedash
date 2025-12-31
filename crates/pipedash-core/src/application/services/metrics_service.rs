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
use crate::infrastructure::database::MetricsRepository;
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
                run.run_number > last_processed
                    && matches!(
                        run.status,
                        PipelineStatus::Success
                            | PipelineStatus::Failed
                            | PipelineStatus::Cancelled
                            | PipelineStatus::Skipped
                    )
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
            };

            if let Some(duration_seconds) = run.duration_seconds {
                metrics.push(MetricEntry {
                    id: 0,
                    pipeline_id: pipeline_id.to_string(),
                    run_number: run.run_number,
                    timestamp: run.started_at,
                    metric_type: MetricType::RunDuration,
                    value: duration_seconds as f64,
                    metadata: Some(metadata.to_json()),
                    created_at: Utc::now(),
                    run_hash: Some(run_hash.clone()),
                });
            }

            let success_value = match run.status {
                PipelineStatus::Success => 100.0,
                PipelineStatus::Failed | PipelineStatus::Cancelled => 0.0,
                _ => continue,
            };

            metrics.push(MetricEntry {
                id: 0,
                pipeline_id: pipeline_id.to_string(),
                run_number: run.run_number,
                timestamp: run.started_at,
                metric_type: MetricType::SuccessRate,
                value: success_value,
                metadata: Some(metadata.to_json()),
                created_at: Utc::now(),
                run_hash: Some(run_hash.clone()),
            });

            metrics.push(MetricEntry {
                id: 0,
                pipeline_id: pipeline_id.to_string(),
                run_number: run.run_number,
                timestamp: run.started_at,
                metric_type: MetricType::RunFrequency,
                value: 1.0,
                metadata: Some(metadata.to_json()),
                created_at: Utc::now(),
                run_hash: Some(run_hash),
            });
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
