use std::collections::HashSet;

use chrono::{
    DateTime,
    Utc,
};

use super::MetricsService;
use crate::domain::{
    DomainError,
    DomainResult,
    MetricEntry,
    MetricMetadata,
    MetricType,
    PipelineRun,
    PipelineStatus,
};
use crate::infrastructure::database::RecordMetrics;
use crate::infrastructure::deduplication::hash_pipeline_run;

impl MetricsService {
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
            self.store_new_runs(pipeline_id, runs, retention_cutoff)
                .await?
        } else {
            0
        };

        Ok(refreshed + stored)
    }

    async fn store_new_runs(
        &self, pipeline_id: &str, runs: &[PipelineRun], retention_cutoff: DateTime<Utc>,
    ) -> DomainResult<usize> {
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

        let candidates: Vec<_> = runs
            .iter()
            .filter(|run| {
                !is_snapshot(run) && !is_native_history(run) && run.started_at >= retention_cutoff
            })
            .collect();
        let oldest_unfinished = candidates
            .iter()
            .filter(|run| !is_final(&run.status))
            .map(|run| run.run_number)
            .min();
        let finished: Vec<_> = candidates
            .into_iter()
            .filter(|run| is_final(&run.status))
            .collect();
        let stored = match finished.iter().map(|run| run.run_number).min() {
            Some(oldest) => {
                self.repository
                    .get_stored_run_numbers_after(pipeline_id, oldest - 1)
                    .await?
            }
            None => HashSet::new(),
        };
        let mut new_runs: Vec<_> = finished
            .iter()
            .copied()
            .filter(|run| {
                !stored.contains(&run.run_number)
                    && record_metrics(pipeline_id, run)
                        .entries
                        .iter()
                        .any(Option::is_some)
            })
            .collect();
        new_runs.sort_by_key(|run| run.run_number);

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

        let processed_through = if runs_to_process.len() < new_runs.len() {
            runs_to_process.last().map(|run| run.run_number)
        } else {
            finished.iter().map(|run| run.run_number).max()
        }
        .unwrap_or(last_processed);
        let max_run_number = oldest_unfinished
            .map_or(processed_through, |run_number| {
                processed_through.min(run_number - 1)
            })
            .max(last_processed);

        let mut metrics = Vec::new();
        for run in runs_to_process {
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

    let run_hash = hash_pipeline_run(run);

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
