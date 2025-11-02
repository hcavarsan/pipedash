use chrono::{
    DateTime,
    Duration,
    Utc,
};
use sqlx::{
    Row as SqlxRow,
    SqlitePool,
};

use crate::domain::{
    AggregatedMetric,
    AggregatedMetrics,
    AggregationPeriod,
    AggregationType,
    DomainError,
    DomainResult,
    GlobalMetricsConfig,
    MetricEntry,
    MetricType,
    MetricsConfig,
    MetricsQuery,
    MetricsStats,
    PipelineMetricsStats,
};

pub struct MetricsRepository {
    pool: SqlitePool,
}

impl MetricsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_global_config(&self) -> DomainResult<GlobalMetricsConfig> {
        let result = sqlx::query(
            "SELECT enabled, default_retention_days, updated_at FROM metrics_global_config WHERE id = 1"
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        match result {
            Some(row) => {
                let enabled: i64 = row
                    .try_get(0)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                let default_retention_days: i64 = row
                    .try_get(1)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                let updated_at_str: String = row
                    .try_get(2)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                Ok(GlobalMetricsConfig {
                    enabled: enabled != 0,
                    default_retention_days,
                    updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            }
            None => Ok(GlobalMetricsConfig::default()),
        }
    }

    pub async fn update_global_config(
        &self, enabled: bool, default_retention_days: i64,
    ) -> DomainResult<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE metrics_global_config SET enabled = ?, default_retention_days = ?, updated_at = ? WHERE id = 1"
        )
        .bind(enabled as i64)
        .bind(default_retention_days)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_pipeline_config(
        &self, pipeline_id: &str,
    ) -> DomainResult<Option<MetricsConfig>> {
        let result = sqlx::query(
            "SELECT pipeline_id, enabled, retention_days, created_at, updated_at FROM metrics_config WHERE pipeline_id = ?"
        )
        .bind(pipeline_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        match result {
            Some(row) => Ok(Some(self.metrics_config_from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn upsert_pipeline_config(
        &self, pipeline_id: &str, enabled: bool, retention_days: i64,
    ) -> DomainResult<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO metrics_config (pipeline_id, enabled, retention_days, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(pipeline_id) DO UPDATE SET
                 enabled = excluded.enabled,
                 retention_days = excluded.retention_days,
                 updated_at = excluded.updated_at"
        )
        .bind(pipeline_id)
        .bind(enabled as i64)
        .bind(retention_days)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_last_processed_run(&self, pipeline_id: &str) -> DomainResult<Option<i64>> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT last_processed_run_number FROM metrics_processing_state WHERE pipeline_id = ?",
        )
        .bind(pipeline_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(result)
    }

    pub async fn update_last_processed_run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO metrics_processing_state (pipeline_id, last_processed_run_number, last_processed_at, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(pipeline_id) DO UPDATE SET
                 last_processed_run_number = excluded.last_processed_run_number,
                 last_processed_at = excluded.last_processed_at,
                 updated_at = excluded.updated_at"
        )
        .bind(pipeline_id)
        .bind(run_number)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn reset_processing_state(&self, pipeline_id: &str) -> DomainResult<()> {
        sqlx::query("DELETE FROM metrics_processing_state WHERE pipeline_id = ?")
            .bind(pipeline_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        eprintln!(
            "[METRICS] Reset processing state for pipeline: {}",
            pipeline_id
        );
        Ok(())
    }

    pub async fn reset_all_processing_states(&self) -> DomainResult<()> {
        sqlx::query("DELETE FROM metrics_processing_state")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        eprintln!("[METRICS] Reset all processing states");
        Ok(())
    }

    pub async fn insert_metrics_batch(&self, metrics: Vec<MetricEntry>) -> DomainResult<usize> {
        if metrics.is_empty() {
            return Ok(0);
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let mut inserted = 0;
        let mut skipped = 0;
        for metric in metrics {
            let metadata_json = metric
                .metadata
                .map(|m| m.to_string())
                .unwrap_or_else(|| "null".to_string());

            let result = sqlx::query(
                "INSERT OR IGNORE INTO pipeline_metrics (pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&metric.pipeline_id)
            .bind(metric.run_number)
            .bind(metric.timestamp.to_rfc3339())
            .bind(metric.metric_type.as_str())
            .bind(metric.value)
            .bind(&metadata_json)
            .bind(metric.created_at.to_rfc3339())
            .bind(&metric.run_hash)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            if result.rows_affected() > 0 {
                inserted += 1;
            } else {
                skipped += 1;
            }
        }

        if skipped > 0 {
            eprintln!("[METRICS] Skipped {} duplicate metrics", skipped);
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(inserted)
    }

    pub async fn query_metrics(&self, query: MetricsQuery) -> DomainResult<Vec<MetricEntry>> {
        let mut sql = String::from("SELECT id, pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash FROM pipeline_metrics WHERE 1=1");
        let mut bind_values: Vec<String> = Vec::new();

        if let Some(pipeline_id) = &query.pipeline_id {
            sql.push_str(" AND pipeline_id = ?");
            bind_values.push(pipeline_id.clone());
        }

        if let Some(metric_type) = &query.metric_type {
            sql.push_str(" AND metric_type = ?");
            bind_values.push(metric_type.as_str().to_string());
        }

        if let Some(start_date) = &query.start_date {
            sql.push_str(" AND timestamp >= ?");
            bind_values.push(start_date.to_rfc3339());
        }

        if let Some(end_date) = &query.end_date {
            sql.push_str(" AND timestamp <= ?");
            bind_values.push(end_date.to_rfc3339());
        }

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let mut query_builder = sqlx::query(&sql);
        for value in &bind_values {
            query_builder = query_builder.bind(value);
        }

        let rows = query_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let metrics = rows
            .iter()
            .map(|row| self.metric_entry_from_row(row))
            .collect::<DomainResult<Vec<_>>>()?;

        Ok(metrics)
    }

    pub async fn query_aggregated_metrics(
        &self, query: MetricsQuery,
    ) -> DomainResult<AggregatedMetrics> {
        let metric_type = query
            .metric_type
            .ok_or_else(|| DomainError::InvalidConfig("metric_type is required".to_string()))?;
        let aggregation_period = query.aggregation_period.ok_or_else(|| {
            DomainError::InvalidConfig("aggregation_period is required".to_string())
        })?;
        let aggregation_type = query.aggregation_type.unwrap_or(AggregationType::Avg);

        let (_time_format, period_select) = match aggregation_period {
            AggregationPeriod::Hourly => (
                "%Y-%m-%d %H:00:00",
                "datetime(strftime('%Y-%m-%d %H:00:00', timestamp))",
            ),
            AggregationPeriod::Daily => ("%Y-%m-%d", "date(timestamp)"),
            AggregationPeriod::Weekly => ("%Y-%W", "date(timestamp, 'weekday 0', '-6 days')"),
            AggregationPeriod::Monthly => ("%Y-%m", "date(timestamp, 'start of month')"),
        };

        let use_percentile = matches!(
            aggregation_type,
            AggregationType::P95 | AggregationType::P99
        );

        let metrics = if use_percentile {
            self.calculate_percentile_metrics(
                &query,
                metric_type,
                aggregation_period,
                aggregation_type,
                period_select,
            )
            .await?
        } else {
            let aggregation_select = match aggregation_type {
                AggregationType::Avg => "AVG(value)",
                AggregationType::Sum => "SUM(value)",
                AggregationType::Min => "MIN(value)",
                AggregationType::Max => "MAX(value)",
                _ => unreachable!(),
            };

            let mut sql = format!(
                "SELECT
                    {} as period,
                    {} as agg_value,
                    COUNT(*) as count,
                    MIN(value) as min_value,
                    MAX(value) as max_value,
                    AVG(value) as avg_value
                 FROM pipeline_metrics
                 WHERE metric_type = ?",
                period_select, aggregation_select
            );

            let mut bind_values: Vec<String> = vec![metric_type.as_str().to_string()];

            if let Some(pipeline_id) = &query.pipeline_id {
                sql.push_str(" AND pipeline_id = ?");
                bind_values.push(pipeline_id.clone());
            }

            if let Some(start_date) = &query.start_date {
                sql.push_str(" AND timestamp >= ?");
                bind_values.push(start_date.to_rfc3339());
            }

            if let Some(end_date) = &query.end_date {
                sql.push_str(" AND timestamp <= ?");
                bind_values.push(end_date.to_rfc3339());
            }

            sql.push_str(" GROUP BY period ORDER BY period ASC");

            if let Some(limit) = query.limit {
                sql.push_str(&format!(" LIMIT {}", limit));
            }

            let mut query_builder = sqlx::query(&sql);
            for value in &bind_values {
                query_builder = query_builder.bind(value);
            }

            let rows = query_builder
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            rows.iter()
                .map(|row| {
                    let period_str: String = row
                        .try_get(0)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let timestamp = self.parse_period_timestamp(&period_str, aggregation_period);

                    Ok(AggregatedMetric {
                        timestamp,
                        value: row
                            .try_get::<f64, _>(1)
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
                        count: row
                            .try_get(2)
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
                        min: row.try_get(3).ok(),
                        max: row.try_get(4).ok(),
                        avg: row
                            .try_get(5)
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
                    })
                })
                .collect::<DomainResult<Vec<_>>>()?
        };

        let total_count = metrics.len();

        Ok(AggregatedMetrics {
            metrics,
            total_count,
            metric_type,
            aggregation_period,
        })
    }

    async fn calculate_percentile_metrics(
        &self, query: &MetricsQuery, metric_type: MetricType,
        aggregation_period: AggregationPeriod, aggregation_type: AggregationType,
        period_select: &str,
    ) -> DomainResult<Vec<AggregatedMetric>> {
        let percentile = if matches!(aggregation_type, AggregationType::P95) {
            0.95
        } else {
            0.99
        };

        let mut sql = format!(
            "SELECT
                {} as period,
                value
             FROM pipeline_metrics
             WHERE metric_type = ?",
            period_select
        );

        let mut bind_values: Vec<String> = vec![metric_type.as_str().to_string()];

        if let Some(pipeline_id) = &query.pipeline_id {
            sql.push_str(" AND pipeline_id = ?");
            bind_values.push(pipeline_id.clone());
        }

        if let Some(start_date) = &query.start_date {
            sql.push_str(" AND timestamp >= ?");
            bind_values.push(start_date.to_rfc3339());
        }

        if let Some(end_date) = &query.end_date {
            sql.push_str(" AND timestamp <= ?");
            bind_values.push(end_date.to_rfc3339());
        }

        sql.push_str(" ORDER BY period, value");

        let mut query_builder = sqlx::query(&sql);
        for value in &bind_values {
            query_builder = query_builder.bind(value);
        }

        let rows = query_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let data: Vec<(String, f64)> = rows
            .iter()
            .map(|row| {
                let period: String = row
                    .try_get(0)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                let value: f64 = row
                    .try_get(1)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                Ok((period, value))
            })
            .collect::<DomainResult<Vec<_>>>()?;

        let mut period_values: std::collections::BTreeMap<String, Vec<f64>> =
            std::collections::BTreeMap::new();

        for (period, value) in data {
            if !value.is_nan() && value.is_finite() {
                period_values.entry(period).or_default().push(value);
            }
        }

        let metrics: Vec<AggregatedMetric> = period_values
            .into_iter()
            .filter_map(|(period_str, mut values)| {
                if values.is_empty() {
                    return None;
                }

                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

                let count = values.len();
                let min = values.first().copied();
                let max = values.last().copied();
                let sum: f64 = values.iter().sum();
                let avg = sum / count as f64;

                let percentile_value = self.calculate_percentile(&values, percentile);

                let timestamp = self.parse_period_timestamp(&period_str, aggregation_period);

                Some(AggregatedMetric {
                    timestamp,
                    value: percentile_value,
                    count: count as i64,
                    min,
                    max,
                    avg,
                })
            })
            .collect();

        let mut sorted_metrics = metrics;
        sorted_metrics.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        if let Some(limit) = query.limit {
            sorted_metrics.truncate(limit);
        }

        Ok(sorted_metrics)
    }

    fn calculate_percentile(&self, sorted_values: &[f64], percentile: f64) -> f64 {
        if sorted_values.is_empty() {
            return 0.0;
        }

        if sorted_values.len() == 1 {
            return sorted_values[0];
        }

        let position = percentile * (sorted_values.len() - 1) as f64;
        let lower_index = position.floor() as usize;
        let upper_index = position.ceil() as usize;

        if lower_index == upper_index {
            sorted_values[lower_index]
        } else {
            let lower_value = sorted_values[lower_index];
            let upper_value = sorted_values[upper_index];
            let fraction = position - lower_index as f64;
            lower_value + (upper_value - lower_value) * fraction
        }
    }

    fn parse_period_timestamp(
        &self, period_str: &str, aggregation_period: AggregationPeriod,
    ) -> DateTime<Utc> {
        if aggregation_period == AggregationPeriod::Weekly {
            DateTime::parse_from_str(
                &format!("{} 00:00:00 +0000", period_str),
                "%Y-%m-%d %H:%M:%S %z",
            )
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
        } else {
            let datetime_str = if period_str.contains(' ') {
                format!("{} +0000", period_str)
            } else {
                format!("{} 00:00:00 +0000", period_str)
            };

            DateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M:%S %z")
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        }
    }

    pub async fn delete_old_metrics(&self, pipeline_id: Option<&str>) -> DomainResult<usize> {
        let global_config = self.get_global_config().await?;
        let mut total_deleted = 0;

        if let Some(pid) = pipeline_id {
            let config = self.get_pipeline_config(pid).await?;
            let retention_days = config
                .as_ref()
                .map(|c| c.retention_days)
                .unwrap_or(global_config.default_retention_days);

            let cutoff_date = Utc::now() - Duration::days(retention_days);

            let result = sqlx::query(
                "DELETE FROM pipeline_metrics
                 WHERE pipeline_id = ?
                 AND datetime(timestamp) < datetime(?)",
            )
            .bind(pid)
            .bind(cutoff_date.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            total_deleted += result.rows_affected() as usize;
        } else {
            let pipeline_ids = sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT pipeline_id FROM pipeline_metrics",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            for pid in pipeline_ids {
                let config = self.get_pipeline_config(&pid).await?;
                let retention_days = config
                    .as_ref()
                    .map(|c| c.retention_days)
                    .unwrap_or(global_config.default_retention_days);

                let cutoff_date = Utc::now() - Duration::days(retention_days);

                let result = sqlx::query(
                    "DELETE FROM pipeline_metrics
                     WHERE pipeline_id = ?
                     AND datetime(timestamp) < datetime(?)",
                )
                .bind(&pid)
                .bind(cutoff_date.to_rfc3339())
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                let deleted = result.rows_affected() as usize;
                if deleted > 0 {
                    eprintln!(
                        "[METRICS] Cleaned {} old metrics for pipeline {} (retention: {} days)",
                        deleted, pid, retention_days
                    );
                }

                total_deleted += deleted;
            }
        }

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE metrics_storage_info SET last_cleanup_at = ?, updated_at = ? WHERE id = 1",
        )
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if total_deleted > 100 {
            eprintln!(
                "[METRICS] Running incremental vacuum after deleting {} metrics",
                total_deleted
            );

            sqlx::query("PRAGMA incremental_vacuum(100)")
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            eprintln!("[METRICS] Incremental vacuum complete");
        }

        Ok(total_deleted)
    }

    pub async fn flush_metrics(&self, pipeline_id: Option<&str>) -> DomainResult<usize> {
        let result = if let Some(pid) = pipeline_id {
            sqlx::query("DELETE FROM pipeline_metrics WHERE pipeline_id = ?")
                .bind(pid)
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
        } else {
            sqlx::query("DELETE FROM pipeline_metrics")
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
        };

        let deleted_count = result.rows_affected() as usize;

        if deleted_count > 0 {
            eprintln!(
                "[METRICS] Running VACUUM after flushing {} metrics",
                deleted_count
            );
            sqlx::query("VACUUM")
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            eprintln!("[METRICS] VACUUM complete");
        }

        Ok(deleted_count)
    }

    pub async fn calculate_storage_stats(&self) -> DomainResult<MetricsStats> {
        let total_metrics_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_metrics")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        const ESTIMATED_ROW_SIZE_BYTES: i64 = 250;
        const OVERHEAD_MULTIPLIER: f64 = 1.2;

        let estimated_size_bytes =
            ((total_metrics_count * ESTIMATED_ROW_SIZE_BYTES) as f64 * OVERHEAD_MULTIPLIER) as i64;
        let estimated_size_mb = estimated_size_bytes as f64 / 1024.0 / 1024.0;

        let last_cleanup_at: Option<DateTime<Utc>> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT last_cleanup_at FROM metrics_storage_info WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        let rows = sqlx::query(
            "SELECT
                pm.pipeline_id,
                COUNT(*) as count,
                MIN(pm.timestamp) as oldest,
                MAX(pm.timestamp) as newest
             FROM pipeline_metrics pm
             GROUP BY pm.pipeline_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let by_pipeline: Vec<PipelineMetricsStats> = rows
            .iter()
            .map(|row| {
                let pipeline_id: String = row
                    .try_get(0)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                let oldest_str: Option<String> = row
                    .try_get(2)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                let newest_str: Option<String> = row
                    .try_get(3)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                Ok(PipelineMetricsStats {
                    pipeline_id: pipeline_id.clone(),
                    pipeline_name: pipeline_id,
                    metrics_count: row
                        .try_get(1)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
                    oldest_metric: oldest_str.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    }),
                    newest_metric: newest_str.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    }),
                })
            })
            .collect::<DomainResult<Vec<_>>>()?;

        Ok(MetricsStats {
            total_metrics_count,
            estimated_size_bytes,
            estimated_size_mb,
            last_cleanup_at,
            updated_at: Utc::now(),
            by_pipeline,
        })
    }

    fn metrics_config_from_row(
        &self, row: &sqlx::sqlite::SqliteRow,
    ) -> DomainResult<MetricsConfig> {
        let enabled: i64 = row
            .try_get(1)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let created_at_str: String = row
            .try_get(3)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let updated_at_str: String = row
            .try_get(4)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(MetricsConfig {
            pipeline_id: row
                .try_get(0)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            enabled: enabled != 0,
            retention_days: row
                .try_get(2)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }

    fn metric_entry_from_row(&self, row: &sqlx::sqlite::SqliteRow) -> DomainResult<MetricEntry> {
        let metric_type_str: String = row
            .try_get(4)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let metric_type = MetricType::from_str(&metric_type_str).unwrap_or(MetricType::RunDuration);

        let metadata_str: Option<String> = row
            .try_get(6)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let metadata: Option<serde_json::Value> =
            metadata_str.and_then(|s| serde_json::from_str(&s).ok());

        let timestamp_str: String = row
            .try_get(3)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let created_at_str: String = row
            .try_get(7)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let run_hash: Option<String> = row.try_get(8).ok();

        Ok(MetricEntry {
            id: row
                .try_get(0)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            pipeline_id: row
                .try_get(1)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            run_number: row
                .try_get(2)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            metric_type,
            value: row
                .try_get(5)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            metadata,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            run_hash,
        })
    }
}
