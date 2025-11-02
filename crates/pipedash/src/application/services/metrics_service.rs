use std::sync::Arc;

use chrono::Utc;

use crate::domain::{
    AggregatedMetrics,
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
    metrics_repository: Arc<MetricsRepository>,
}

impl MetricsService {
    pub fn new(metrics_repository: Arc<MetricsRepository>) -> Self {
        Self { metrics_repository }
    }

    pub async fn get_global_config(&self) -> DomainResult<GlobalMetricsConfig> {
        self.metrics_repository.get_global_config().await
    }

    pub async fn update_global_config(
        &self, enabled: bool, default_retention_days: i64,
    ) -> DomainResult<()> {
        if default_retention_days < 1 {
            return Err(crate::domain::DomainError::InvalidConfig(
                "Retention days must be at least 1".to_string(),
            ));
        }

        // Get the previous state to check if we're enabling metrics
        let previous_config = self.metrics_repository.get_global_config().await?;

        // Update the configuration
        self.metrics_repository
            .update_global_config(enabled, default_retention_days)
            .await?;

        // If we're enabling metrics that were previously disabled, reset all processing
        // states
        if enabled && !previous_config.enabled {
            eprintln!("[METRICS] Global metrics re-enabled - resetting all processing states");
            self.metrics_repository
                .reset_all_processing_states()
                .await?;
        }

        Ok(())
    }

    pub async fn get_pipeline_config(&self, pipeline_id: &str) -> DomainResult<MetricsConfig> {
        if let Some(config) = self
            .metrics_repository
            .get_pipeline_config(pipeline_id)
            .await?
        {
            Ok(config)
        } else {
            let global_config = self.metrics_repository.get_global_config().await?;
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
            return Err(crate::domain::DomainError::InvalidConfig(
                "Retention days must be at least 1".to_string(),
            ));
        }

        // Get the previous state to check if we're enabling metrics
        let previous_config = self.get_pipeline_config(pipeline_id).await?;

        // Update the configuration
        self.metrics_repository
            .upsert_pipeline_config(pipeline_id, enabled, retention_days)
            .await?;

        // If we're enabling metrics that were previously disabled, reset the processing
        // state for this pipeline
        if enabled && !previous_config.enabled {
            eprintln!(
                "[METRICS] Metrics re-enabled for pipeline {} - resetting processing state",
                pipeline_id
            );
            self.metrics_repository
                .reset_processing_state(pipeline_id)
                .await?;
        }

        Ok(())
    }

    pub async fn extract_and_store_metrics(
        &self, pipeline_id: &str, runs: &[PipelineRun],
    ) -> DomainResult<usize> {
        if runs.is_empty() {
            return Ok(0);
        }

        let config = self.get_pipeline_config(pipeline_id).await?;
        if !config.enabled {
            return Ok(0);
        }

        // Get last processed run to avoid duplicates
        let last_processed = self
            .metrics_repository
            .get_last_processed_run(pipeline_id)
            .await?
            .unwrap_or(0);

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

        if new_runs.is_empty() {
            return Ok(0);
        }

        const MAX_BATCH_SIZE: usize = 200;
        let runs_to_process = if new_runs.len() > MAX_BATCH_SIZE {
            eprintln!(
                "[METRICS] Limiting batch: {} new runs available, processing first {} for {}",
                new_runs.len(),
                MAX_BATCH_SIZE,
                pipeline_id
            );
            &new_runs[..MAX_BATCH_SIZE]
        } else {
            eprintln!(
                "[METRICS] Processing {} new runs for {} (last processed: run #{})",
                new_runs.len(),
                pipeline_id,
                last_processed
            );
            &new_runs[..]
        };

        let mut metrics = Vec::new();
        let mut max_run_number = last_processed;

        for run in runs_to_process {
            max_run_number = max_run_number.max(run.run_number);

            let status_str = match run.status {
                PipelineStatus::Success => "success",
                PipelineStatus::Failed => "failed",
                PipelineStatus::Running => "running",
                PipelineStatus::Pending => "pending",
                PipelineStatus::Cancelled => "cancelled",
                PipelineStatus::Skipped => "skipped",
            };

            let run_hash = hash_pipeline_run(
                run.run_number,
                status_str,
                &run.branch,
                &run.started_at.to_rfc3339(),
                run.duration_seconds,
                &run.commit_sha,
            );

            let metadata = MetricMetadata {
                status: Some(status_str.to_string()),
                branch: Some(run.branch.clone()),
                repository: None,
                actor: Some(run.actor.clone()),
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
        let inserted = self
            .metrics_repository
            .insert_metrics_batch(metrics)
            .await?;
        let insert_duration = start.elapsed();

        if insert_duration.as_secs() > 2 {
            eprintln!(
                "[METRICS] WARNING: Slow insert took {:?} for {} metrics",
                insert_duration, inserted
            );
        }

        self.metrics_repository
            .update_last_processed_run(pipeline_id, max_run_number)
            .await?;

        eprintln!(
            "[METRICS] Stored {} metrics for {} in {:?} (now at run #{})",
            inserted, pipeline_id, insert_duration, max_run_number
        );

        Ok(inserted)
    }

    pub async fn query_metrics(&self, query: MetricsQuery) -> DomainResult<Vec<MetricEntry>> {
        self.metrics_repository.query_metrics(query).await
    }

    pub async fn query_aggregated_metrics(
        &self, query: MetricsQuery,
    ) -> DomainResult<AggregatedMetrics> {
        self.metrics_repository
            .query_aggregated_metrics(query)
            .await
    }

    pub async fn cleanup_old_metrics(&self) -> DomainResult<usize> {
        self.metrics_repository.delete_old_metrics(None).await
    }

    pub async fn get_storage_stats(&self) -> DomainResult<MetricsStats> {
        self.metrics_repository.calculate_storage_stats().await
    }

    pub async fn flush_metrics(&self, pipeline_id: Option<String>) -> DomainResult<usize> {
        let result = self
            .metrics_repository
            .flush_metrics(pipeline_id.as_deref())
            .await?;

        if let Some(ref pipeline) = pipeline_id {
            self.metrics_repository
                .reset_processing_state(pipeline)
                .await?;
        } else {
            self.metrics_repository
                .reset_all_processing_states()
                .await?;
        }

        Ok(result)
    }
}
