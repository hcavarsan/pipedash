use chrono::{
    DateTime,
    Duration,
    Utc,
};
use sqlx::Row as SqlxRow;

use super::DatabasePool;
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
    MetricsConfigExport,
    MetricsQuery,
    MetricsStats,
    PipelineMetricsStats,
};

pub struct MetricsRepository {
    pool: DatabasePool,
}

impl MetricsRepository {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: DatabasePool::Sqlite(pool),
        }
    }

    pub fn new_from_pool(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn placeholder(&self, index: usize) -> String {
        match &self.pool {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${}", index),
        }
    }

    fn datetime_now(&self) -> &'static str {
        match &self.pool {
            DatabasePool::Sqlite(_) => "datetime('now')",
            DatabasePool::Postgres(_) => "NOW()",
        }
    }

    pub async fn count_metrics_for_pipeline(&self, pipeline_id: &str) -> DomainResult<i64> {
        match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM pipeline_metrics WHERE pipeline_id = ?",
            )
            .bind(pipeline_id)
            .fetch_one(p)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string())),
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM pipeline_metrics WHERE pipeline_id = $1",
            )
            .bind(pipeline_id)
            .fetch_one(p)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string())),
        }
    }

    pub async fn get_global_config(&self) -> DomainResult<GlobalMetricsConfig> {
        let result = match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query(
                    "SELECT enabled, default_retention_days, updated_at FROM metrics_global_config WHERE id = 1",
                )
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                .map(|row| self.parse_global_config_row_sqlite(&row))
                .transpose()?
            }
            DatabasePool::Postgres(p) => {
                sqlx::query(
                    "SELECT enabled, default_retention_days::BIGINT, updated_at FROM metrics_global_config WHERE id = 1",
                )
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                .map(|row| self.parse_global_config_row_postgres(&row))
                .transpose()?
            }
        };

        Ok(result.unwrap_or_default())
    }

    fn parse_global_config_row_sqlite(
        &self, row: &sqlx::sqlite::SqliteRow,
    ) -> DomainResult<GlobalMetricsConfig> {
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

    fn parse_global_config_row_postgres(
        &self, row: &sqlx::postgres::PgRow,
    ) -> DomainResult<GlobalMetricsConfig> {
        let enabled: bool = row
            .try_get(0)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let default_retention_days: i64 = row
            .try_get(1)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let updated_at: DateTime<Utc> = row
            .try_get(2)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(GlobalMetricsConfig {
            enabled,
            default_retention_days,
            updated_at,
        })
    }

    pub async fn update_global_config(
        &self, enabled: bool, default_retention_days: i64,
    ) -> DomainResult<()> {
        let sql = format!(
            "UPDATE metrics_global_config SET enabled = {}, default_retention_days = {}, updated_at = {} WHERE id = 1",
            self.placeholder(1),
            self.placeholder(2),
            self.datetime_now()
        );

        match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query(&sql)
                    .bind(enabled as i64)
                    .bind(default_retention_days)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query(&sql)
                    .bind(enabled)
                    .bind(default_retention_days)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn get_pipeline_config(
        &self, pipeline_id: &str,
    ) -> DomainResult<Option<MetricsConfig>> {
        match &self.pool {
            DatabasePool::Sqlite(p) => {
                let result = sqlx::query(
                    "SELECT pipeline_id, enabled, retention_days, created_at, updated_at FROM metrics_config WHERE pipeline_id = ?",
                )
                .bind(pipeline_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                match result {
                    Some(row) => Ok(Some(self.metrics_config_from_sqlite_row(&row)?)),
                    None => Ok(None),
                }
            }
            DatabasePool::Postgres(p) => {
                let result = sqlx::query(
                    "SELECT pipeline_id, enabled, retention_days::BIGINT, created_at, updated_at FROM metrics_config WHERE pipeline_id = $1",
                )
                .bind(pipeline_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                match result {
                    Some(row) => Ok(Some(self.metrics_config_from_postgres_row(&row)?)),
                    None => Ok(None),
                }
            }
        }
    }

    pub async fn upsert_pipeline_config(
        &self, pipeline_id: &str, enabled: bool, retention_days: i64,
    ) -> DomainResult<()> {
        match &self.pool {
            DatabasePool::Sqlite(p) => {
                let now = Utc::now().to_rfc3339();
                sqlx::query(
                    "INSERT INTO metrics_config (pipeline_id, enabled, retention_days, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT(pipeline_id) DO UPDATE SET
                         enabled = excluded.enabled,
                         retention_days = excluded.retention_days,
                         updated_at = excluded.updated_at",
                )
                .bind(pipeline_id)
                .bind(enabled as i64)
                .bind(retention_days)
                .bind(&now)
                .bind(&now)
                .execute(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                let now = Utc::now();
                sqlx::query(
                    "INSERT INTO metrics_config (pipeline_id, enabled, retention_days, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT(pipeline_id) DO UPDATE SET
                         enabled = excluded.enabled,
                         retention_days = excluded.retention_days,
                         updated_at = excluded.updated_at",
                )
                .bind(pipeline_id)
                .bind(enabled)
                .bind(retention_days)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn get_last_processed_run(&self, pipeline_id: &str) -> DomainResult<Option<i64>> {
        let result = match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT last_processed_run_number FROM metrics_processing_state WHERE pipeline_id = ?",
                )
                .bind(pipeline_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT last_processed_run_number::BIGINT FROM metrics_processing_state WHERE pipeline_id = $1",
                )
                .bind(pipeline_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
        };

        Ok(result)
    }

    pub async fn update_last_processed_run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<()> {
        match &self.pool {
            DatabasePool::Sqlite(p) => {
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
                .execute(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                let now = Utc::now();
                sqlx::query(
                    "INSERT INTO metrics_processing_state (pipeline_id, last_processed_run_number, last_processed_at, updated_at)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT(pipeline_id) DO UPDATE SET
                         last_processed_run_number = excluded.last_processed_run_number,
                         last_processed_at = excluded.last_processed_at,
                         updated_at = excluded.updated_at"
                )
                .bind(pipeline_id)
                .bind(run_number)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn reset_processing_state(&self, pipeline_id: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM metrics_processing_state WHERE pipeline_id = {}",
            self.placeholder(1)
        );

        match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query(&sql)
                    .bind(pipeline_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query(&sql)
                    .bind(pipeline_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        tracing::debug!(pipeline_id = %pipeline_id, "Reset processing state for pipeline");
        Ok(())
    }

    pub async fn reset_all_processing_states(&self) -> DomainResult<()> {
        match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query("DELETE FROM metrics_processing_state")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query("DELETE FROM metrics_processing_state")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        tracing::debug!("Reset all processing states");
        Ok(())
    }

    pub async fn insert_metrics_batch(&self, metrics: Vec<MetricEntry>) -> DomainResult<usize> {
        if metrics.is_empty() {
            return Ok(0);
        }

        let start = std::time::Instant::now();
        let metrics_count = metrics.len();
        let mut total_inserted = 0;

        const BATCH_SIZE: usize = 100;

        match &self.pool {
            DatabasePool::Sqlite(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for chunk in metrics.chunks(BATCH_SIZE) {
                    let prepared_data: Vec<_> = chunk
                        .iter()
                        .map(|metric| {
                            let metadata_json = metric
                                .metadata
                                .as_ref()
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| "null".to_string());

                            (
                                metric.pipeline_id.clone(),
                                metric.run_number,
                                metric.timestamp.to_rfc3339(),
                                metric.metric_type.as_str().to_string(),
                                metric.value,
                                metadata_json,
                                metric.created_at.to_rfc3339(),
                                metric.run_hash.clone(),
                            )
                        })
                        .collect();

                    let values_clause = prepared_data
                        .iter()
                        .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?)")
                        .collect::<Vec<_>>()
                        .join(", ");

                    let sql = format!(
                        "INSERT OR IGNORE INTO pipeline_metrics (pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash)
                         VALUES {}",
                        values_clause
                    );

                    let mut query = sqlx::query(&sql);
                    for (
                        pipeline_id,
                        run_number,
                        timestamp,
                        metric_type,
                        value,
                        metadata_json,
                        created_at,
                        run_hash,
                    ) in &prepared_data
                    {
                        query = query
                            .bind(pipeline_id)
                            .bind(run_number)
                            .bind(timestamp)
                            .bind(metric_type)
                            .bind(value)
                            .bind(metadata_json)
                            .bind(created_at)
                            .bind(run_hash);
                    }

                    let result = query
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    total_inserted += result.rows_affected() as usize;
                }

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for chunk in metrics.chunks(BATCH_SIZE) {
                    let prepared_data: Vec<_> = chunk
                        .iter()
                        .map(|metric| {
                            let metadata_json = metric
                                .metadata
                                .as_ref()
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| "null".to_string());

                            (
                                metric.pipeline_id.clone(),
                                metric.run_number,
                                metric.timestamp,
                                metric.metric_type.as_str().to_string(),
                                metric.value,
                                metadata_json,
                                metric.created_at,
                                metric.run_hash.clone(),
                            )
                        })
                        .collect();

                    let mut param_idx = 0;
                    let values_clauses: Vec<String> = prepared_data
                        .iter()
                        .map(|_| {
                            let clause = format!(
                                "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                                param_idx + 1,
                                param_idx + 2,
                                param_idx + 3,
                                param_idx + 4,
                                param_idx + 5,
                                param_idx + 6,
                                param_idx + 7,
                                param_idx + 8
                            );
                            param_idx += 8;
                            clause
                        })
                        .collect();

                    let sql = format!(
                        "INSERT INTO pipeline_metrics (pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash)
                         VALUES {}
                         ON CONFLICT DO NOTHING",
                        values_clauses.join(", ")
                    );

                    let mut query = sqlx::query(&sql);
                    for (
                        pipeline_id,
                        run_number,
                        timestamp,
                        metric_type,
                        value,
                        metadata_json,
                        created_at,
                        run_hash,
                    ) in &prepared_data
                    {
                        query = query
                            .bind(pipeline_id)
                            .bind(run_number)
                            .bind(timestamp)
                            .bind(metric_type)
                            .bind(value)
                            .bind(metadata_json)
                            .bind(created_at)
                            .bind(run_hash);
                    }

                    let result = query
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    total_inserted += result.rows_affected() as usize;
                }

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        let skipped = metrics_count - total_inserted;
        if skipped > 0 {
            tracing::debug!(skipped = skipped, "Skipped duplicate metrics");
        }

        let elapsed = start.elapsed();
        tracing::debug!(
            metrics_count = metrics_count,
            inserted = total_inserted,
            skipped = skipped,
            elapsed_ms = elapsed.as_millis(),
            "Inserted metrics batch"
        );

        Ok(total_inserted)
    }

    pub async fn query_metrics(&self, query: MetricsQuery) -> DomainResult<Vec<MetricEntry>> {
        let mut param_idx = 0;
        let base_select = match &self.pool {
            DatabasePool::Sqlite(_) => "SELECT id, pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash FROM pipeline_metrics WHERE 1=1",
            DatabasePool::Postgres(_) => "SELECT id, pipeline_id, run_number::BIGINT, timestamp, metric_type, value, metadata_json, created_at, run_hash FROM pipeline_metrics WHERE 1=1",
        };
        let mut sql = String::from(base_select);

        let mut string_params: Vec<(usize, String)> = Vec::new();
        let mut timestamp_params: Vec<(usize, DateTime<Utc>)> = Vec::new();

        if let Some(pipeline_id) = &query.pipeline_id {
            param_idx += 1;
            sql.push_str(&format!(
                " AND pipeline_id = {}",
                self.placeholder(param_idx)
            ));
            string_params.push((param_idx, pipeline_id.clone()));
        }

        if let Some(metric_type) = &query.metric_type {
            param_idx += 1;
            sql.push_str(&format!(
                " AND metric_type = {}",
                self.placeholder(param_idx)
            ));
            string_params.push((param_idx, metric_type.as_str().to_string()));
        }

        if let Some(start_date) = &query.start_date {
            param_idx += 1;
            sql.push_str(&format!(
                " AND timestamp >= {}",
                self.placeholder(param_idx)
            ));
            timestamp_params.push((param_idx, *start_date));
        }

        if let Some(end_date) = &query.end_date {
            param_idx += 1;
            sql.push_str(&format!(
                " AND timestamp <= {}",
                self.placeholder(param_idx)
            ));
            timestamp_params.push((param_idx, *end_date));
        }

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        match &self.pool {
            DatabasePool::Sqlite(p) => {
                let mut query_builder = sqlx::query(&sql);
                let mut all_params: Vec<(usize, String)> = string_params;
                for (idx, ts) in timestamp_params {
                    all_params.push((idx, ts.to_rfc3339()));
                }
                all_params.sort_by_key(|(idx, _)| *idx);
                for (_, value) in all_params {
                    query_builder = query_builder.bind(value);
                }

                let rows = query_builder
                    .fetch_all(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| self.metric_entry_from_sqlite_row(row))
                    .collect::<DomainResult<Vec<_>>>()
            }
            DatabasePool::Postgres(p) => {
                enum BindValue {
                    Str(String),
                    Timestamp(DateTime<Utc>),
                }
                let mut all_params: Vec<(usize, BindValue)> = Vec::new();
                for (idx, s) in string_params {
                    all_params.push((idx, BindValue::Str(s)));
                }
                for (idx, ts) in timestamp_params {
                    all_params.push((idx, BindValue::Timestamp(ts)));
                }
                all_params.sort_by_key(|(idx, _)| *idx);

                let mut query_builder = sqlx::query(&sql);
                for (_, value) in all_params {
                    match value {
                        BindValue::Str(s) => query_builder = query_builder.bind(s),
                        BindValue::Timestamp(ts) => query_builder = query_builder.bind(ts),
                    }
                }

                let rows = query_builder
                    .fetch_all(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| self.metric_entry_from_postgres_row(row))
                    .collect::<DomainResult<Vec<_>>>()
            }
        }
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

        let period_select = match (&self.pool, aggregation_period) {
            (DatabasePool::Sqlite(_), AggregationPeriod::Hourly) => {
                "datetime(strftime('%Y-%m-%d %H:00:00', timestamp))"
            }
            (DatabasePool::Sqlite(_), AggregationPeriod::Daily) => "date(timestamp)",
            (DatabasePool::Sqlite(_), AggregationPeriod::Weekly) => {
                "date(timestamp, 'weekday 0', '-6 days')"
            }
            (DatabasePool::Sqlite(_), AggregationPeriod::Monthly) => {
                "date(timestamp, 'start of month')"
            }
            (DatabasePool::Postgres(_), AggregationPeriod::Hourly) => {
                "date_trunc('hour', timestamp)"
            }
            (DatabasePool::Postgres(_), AggregationPeriod::Daily) => "date_trunc('day', timestamp)",
            (DatabasePool::Postgres(_), AggregationPeriod::Weekly) => {
                "date_trunc('week', timestamp)"
            }
            (DatabasePool::Postgres(_), AggregationPeriod::Monthly) => {
                "date_trunc('month', timestamp)"
            }
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

            let mut param_idx = 0;
            let mut sql = format!(
                "SELECT
                    {} as period,
                    {} as agg_value,
                    COUNT(*) as count,
                    MIN(value) as min_value,
                    MAX(value) as max_value,
                    AVG(value) as avg_value
                 FROM pipeline_metrics
                 WHERE metric_type = {}",
                period_select,
                aggregation_select,
                {
                    param_idx += 1;
                    self.placeholder(param_idx)
                }
            );

            let mut string_params: Vec<(usize, String)> =
                vec![(1, metric_type.as_str().to_string())];
            let mut timestamp_params: Vec<(usize, DateTime<Utc>)> = Vec::new();

            if let Some(pipeline_id) = &query.pipeline_id {
                param_idx += 1;
                sql.push_str(&format!(
                    " AND pipeline_id = {}",
                    self.placeholder(param_idx)
                ));
                string_params.push((param_idx, pipeline_id.clone()));
            }

            if let Some(start_date) = &query.start_date {
                param_idx += 1;
                sql.push_str(&format!(
                    " AND timestamp >= {}",
                    self.placeholder(param_idx)
                ));
                timestamp_params.push((param_idx, *start_date));
            }

            if let Some(end_date) = &query.end_date {
                param_idx += 1;
                sql.push_str(&format!(
                    " AND timestamp <= {}",
                    self.placeholder(param_idx)
                ));
                timestamp_params.push((param_idx, *end_date));
            }

            sql.push_str(" GROUP BY period ORDER BY period ASC");

            if let Some(limit) = query.limit {
                sql.push_str(&format!(" LIMIT {}", limit));
            }

            match &self.pool {
                DatabasePool::Sqlite(p) => {
                    let mut query_builder = sqlx::query(&sql);
                    let mut all_params: Vec<(usize, String)> = string_params;
                    for (idx, ts) in timestamp_params {
                        all_params.push((idx, ts.to_rfc3339()));
                    }
                    all_params.sort_by_key(|(idx, _)| *idx);
                    for (_, value) in all_params {
                        query_builder = query_builder.bind(value);
                    }

                    let rows = query_builder
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    rows.iter()
                        .map(|row| {
                            let period_str: String = row
                                .try_get(0)
                                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let timestamp =
                                self.parse_period_timestamp(&period_str, aggregation_period);

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
                }
                DatabasePool::Postgres(p) => {
                    enum BindValue {
                        Str(String),
                        Timestamp(DateTime<Utc>),
                    }
                    let mut all_params: Vec<(usize, BindValue)> = Vec::new();
                    for (idx, s) in string_params {
                        all_params.push((idx, BindValue::Str(s)));
                    }
                    for (idx, ts) in timestamp_params {
                        all_params.push((idx, BindValue::Timestamp(ts)));
                    }
                    all_params.sort_by_key(|(idx, _)| *idx);

                    let mut query_builder = sqlx::query(&sql);
                    for (_, value) in all_params {
                        match value {
                            BindValue::Str(s) => query_builder = query_builder.bind(s),
                            BindValue::Timestamp(ts) => query_builder = query_builder.bind(ts),
                        }
                    }

                    let rows = query_builder
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    rows.iter()
                        .map(|row| {
                            let timestamp: DateTime<Utc> = row
                                .try_get(0)
                                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

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
                }
            }
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

        let mut param_idx = 0;
        let mut sql = format!(
            "SELECT {} as period, value FROM pipeline_metrics WHERE metric_type = {}",
            period_select,
            {
                param_idx += 1;
                self.placeholder(param_idx)
            }
        );

        let mut string_params: Vec<(usize, String)> = vec![(1, metric_type.as_str().to_string())];
        let mut timestamp_params: Vec<(usize, DateTime<Utc>)> = Vec::new();

        if let Some(pipeline_id) = &query.pipeline_id {
            param_idx += 1;
            sql.push_str(&format!(
                " AND pipeline_id = {}",
                self.placeholder(param_idx)
            ));
            string_params.push((param_idx, pipeline_id.clone()));
        }

        if let Some(start_date) = &query.start_date {
            param_idx += 1;
            sql.push_str(&format!(
                " AND timestamp >= {}",
                self.placeholder(param_idx)
            ));
            timestamp_params.push((param_idx, *start_date));
        }

        if let Some(end_date) = &query.end_date {
            param_idx += 1;
            sql.push_str(&format!(
                " AND timestamp <= {}",
                self.placeholder(param_idx)
            ));
            timestamp_params.push((param_idx, *end_date));
        }

        sql.push_str(" ORDER BY period, value");

        let data: Vec<(String, f64)> = match &self.pool {
            DatabasePool::Sqlite(p) => {
                let mut query_builder = sqlx::query(&sql);
                let mut all_params: Vec<(usize, String)> = string_params;
                for (idx, ts) in timestamp_params {
                    all_params.push((idx, ts.to_rfc3339()));
                }
                all_params.sort_by_key(|(idx, _)| *idx);
                for (_, value) in all_params {
                    query_builder = query_builder.bind(value);
                }

                let rows = query_builder
                    .fetch_all(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| {
                        let period: String = row
                            .try_get(0)
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        let value: f64 = row
                            .try_get(1)
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        Ok((period, value))
                    })
                    .collect::<DomainResult<Vec<_>>>()?
            }
            DatabasePool::Postgres(p) => {
                enum BindValue {
                    Str(String),
                    Timestamp(DateTime<Utc>),
                }
                let mut all_params: Vec<(usize, BindValue)> = Vec::new();
                for (idx, s) in string_params {
                    all_params.push((idx, BindValue::Str(s)));
                }
                for (idx, ts) in timestamp_params {
                    all_params.push((idx, BindValue::Timestamp(ts)));
                }
                all_params.sort_by_key(|(idx, _)| *idx);

                let mut query_builder = sqlx::query(&sql);
                for (_, value) in all_params {
                    match value {
                        BindValue::Str(s) => query_builder = query_builder.bind(s),
                        BindValue::Timestamp(ts) => query_builder = query_builder.bind(ts),
                    }
                }

                let rows = query_builder
                    .fetch_all(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| {
                        let period: DateTime<Utc> = row
                            .try_get(0)
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        let value: f64 = row
                            .try_get(1)
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        Ok((period.to_rfc3339(), value))
                    })
                    .collect::<DomainResult<Vec<_>>>()?
            }
        };

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
        if let Ok(dt) = DateTime::parse_from_rfc3339(period_str) {
            return dt.with_timezone(&Utc);
        }

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
        let start = std::time::Instant::now();
        let global_config = self.get_global_config().await?;
        let mut total_deleted = 0;

        if let Some(pid) = pipeline_id {
            let config = self.get_pipeline_config(pid).await?;
            let retention_days = config
                .as_ref()
                .map(|c| c.retention_days)
                .unwrap_or(global_config.default_retention_days);

            let cutoff_date = Utc::now() - Duration::days(retention_days);

            let deleted = match &self.pool {
                DatabasePool::Sqlite(p) => {
                    let result = sqlx::query(
                        "DELETE FROM pipeline_metrics WHERE pipeline_id = ? AND datetime(timestamp) < datetime(?)",
                    )
                    .bind(pid)
                    .bind(cutoff_date.to_rfc3339())
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    result.rows_affected() as usize
                }
                DatabasePool::Postgres(p) => {
                    let result = sqlx::query(
                        "DELETE FROM pipeline_metrics WHERE pipeline_id = $1 AND timestamp < $2",
                    )
                    .bind(pid)
                    .bind(cutoff_date)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    result.rows_affected() as usize
                }
            };

            total_deleted += deleted;
        } else {
            use std::collections::HashMap;

            let pipeline_ids = match &self.pool {
                DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, String>(
                    "SELECT DISTINCT pipeline_id FROM pipeline_metrics",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
                DatabasePool::Postgres(p) => sqlx::query_scalar::<_, String>(
                    "SELECT DISTINCT pipeline_id FROM pipeline_metrics",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            };

            if pipeline_ids.is_empty() {
                return Ok(0);
            }

            let configs: HashMap<String, i64> = match &self.pool {
                DatabasePool::Sqlite(p) => sqlx::query_as::<_, (String, i64)>(
                    "SELECT pipeline_id, retention_days FROM metrics_config",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                .into_iter()
                .collect(),
                DatabasePool::Postgres(p) => sqlx::query_as::<_, (String, i64)>(
                    "SELECT pipeline_id, retention_days::BIGINT FROM metrics_config",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                .into_iter()
                .collect(),
            };

            let mut pipelines_by_retention: HashMap<i64, Vec<String>> = HashMap::new();

            for pid in pipeline_ids {
                let retention_days = configs
                    .get(&pid)
                    .copied()
                    .unwrap_or(global_config.default_retention_days);

                pipelines_by_retention
                    .entry(retention_days)
                    .or_default()
                    .push(pid);
            }

            for (retention_days, pids) in pipelines_by_retention {
                let cutoff_date = Utc::now() - Duration::days(retention_days);

                match &self.pool {
                    DatabasePool::Sqlite(p) => {
                        let placeholders = pids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                        let sql = format!(
                            "DELETE FROM pipeline_metrics WHERE pipeline_id IN ({}) AND datetime(timestamp) < datetime(?)",
                            placeholders
                        );

                        let mut query = sqlx::query(&sql);
                        for pid in &pids {
                            query = query.bind(pid);
                        }
                        query = query.bind(cutoff_date.to_rfc3339());

                        let result = query
                            .execute(p)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        total_deleted += result.rows_affected() as usize;
                    }
                    DatabasePool::Postgres(p) => {
                        let mut param_idx = 0;
                        let placeholders = pids
                            .iter()
                            .map(|_| {
                                param_idx += 1;
                                format!("${}", param_idx)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        param_idx += 1;
                        let sql = format!(
                            "DELETE FROM pipeline_metrics WHERE pipeline_id IN ({}) AND timestamp < ${}",
                            placeholders, param_idx
                        );

                        let mut query = sqlx::query(&sql);
                        for pid in &pids {
                            query = query.bind(pid);
                        }
                        query = query.bind(cutoff_date);

                        let result = query
                            .execute(p)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        total_deleted += result.rows_affected() as usize;
                    }
                }
            }
        }

        match &self.pool {
            DatabasePool::Sqlite(p) => {
                let now = Utc::now().to_rfc3339();
                sqlx::query(
                    "UPDATE metrics_storage_info SET last_cleanup_at = ?, updated_at = ? WHERE id = 1",
                )
                .bind(&now)
                .bind(&now)
                .execute(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                let now = Utc::now();
                sqlx::query(
                    "UPDATE metrics_storage_info SET last_cleanup_at = $1, updated_at = $2 WHERE id = 1",
                )
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        let elapsed = start.elapsed();
        tracing::debug!(
            pipeline_id = pipeline_id,
            total_deleted = total_deleted,
            elapsed_ms = elapsed.as_millis(),
            "Deleted old metrics"
        );

        Ok(total_deleted)
    }

    pub async fn flush_metrics(
        &self, pipeline_id: Option<&str>, skip_vacuum: bool,
    ) -> DomainResult<usize> {
        let deleted_count = match &self.pool {
            DatabasePool::Sqlite(p) => {
                let result = if let Some(pid) = pipeline_id {
                    sqlx::query("DELETE FROM pipeline_metrics WHERE pipeline_id = ?")
                        .bind(pid)
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                } else {
                    sqlx::query("DELETE FROM pipeline_metrics")
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                };
                result.rows_affected() as usize
            }
            DatabasePool::Postgres(p) => {
                let result = if let Some(pid) = pipeline_id {
                    sqlx::query("DELETE FROM pipeline_metrics WHERE pipeline_id = $1")
                        .bind(pid)
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                } else {
                    sqlx::query("DELETE FROM pipeline_metrics")
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                };
                result.rows_affected() as usize
            }
        };

        if let DatabasePool::Sqlite(p) = &self.pool {
            if deleted_count > 0 && !skip_vacuum {
                tracing::debug!(
                    deleted = deleted_count,
                    "Running VACUUM after flushing metrics"
                );
                sqlx::query("VACUUM")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                tracing::debug!("VACUUM complete");
            }
        }

        Ok(deleted_count)
    }

    pub async fn calculate_storage_stats(&self) -> DomainResult<MetricsStats> {
        let total_metrics_count: i64 = match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_metrics")
                .fetch_one(p)
                .await
                .unwrap_or(0),
            DatabasePool::Postgres(p) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_metrics")
                    .fetch_one(p)
                    .await
                    .unwrap_or(0)
            }
        };

        const ESTIMATED_ROW_SIZE_BYTES: i64 = 250;
        const OVERHEAD_MULTIPLIER: f64 = 1.2;

        let estimated_size_bytes =
            ((total_metrics_count * ESTIMATED_ROW_SIZE_BYTES) as f64 * OVERHEAD_MULTIPLIER) as i64;
        let estimated_size_mb = estimated_size_bytes as f64 / 1024.0 / 1024.0;

        let last_cleanup_at: Option<DateTime<Utc>> = match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, Option<String>>(
                "SELECT last_cleanup_at FROM metrics_storage_info WHERE id = 1",
            )
            .fetch_optional(p)
            .await
            .ok()
            .flatten()
            .flatten()
            .and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            }),
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
                "SELECT last_cleanup_at FROM metrics_storage_info WHERE id = 1",
            )
            .fetch_optional(p)
            .await
            .ok()
            .flatten()
            .flatten(),
        };

        let by_pipeline = match &self.pool {
            DatabasePool::Sqlite(p) => {
                let rows = sqlx::query(
                    "SELECT pm.pipeline_id, COUNT(*) as count, MIN(pm.timestamp) as oldest, MAX(pm.timestamp) as newest
                     FROM pipeline_metrics pm
                     GROUP BY pm.pipeline_id",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| self.pipeline_metrics_stats_from_sqlite_row(row))
                    .collect::<DomainResult<Vec<_>>>()?
            }
            DatabasePool::Postgres(p) => {
                let rows = sqlx::query(
                    "SELECT pm.pipeline_id, COUNT(*) as count, MIN(pm.timestamp) as oldest, MAX(pm.timestamp) as newest
                     FROM pipeline_metrics pm
                     GROUP BY pm.pipeline_id",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| self.pipeline_metrics_stats_from_postgres_row(row))
                    .collect::<DomainResult<Vec<_>>>()?
            }
        };

        Ok(MetricsStats {
            total_metrics_count,
            estimated_size_bytes,
            estimated_size_mb,
            last_cleanup_at,
            updated_at: Utc::now(),
            by_pipeline,
        })
    }

    pub async fn get_storage_stats(&self) -> DomainResult<MetricsStats> {
        self.calculate_storage_stats().await
    }

    fn metrics_config_from_sqlite_row(
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

    fn metrics_config_from_postgres_row(
        &self, row: &sqlx::postgres::PgRow,
    ) -> DomainResult<MetricsConfig> {
        Ok(MetricsConfig {
            pipeline_id: row
                .try_get(0)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            enabled: row
                .try_get(1)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            retention_days: row
                .try_get(2)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            created_at: row
                .try_get(3)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            updated_at: row
                .try_get(4)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
        })
    }

    fn metric_entry_from_sqlite_row(
        &self, row: &sqlx::sqlite::SqliteRow,
    ) -> DomainResult<MetricEntry> {
        let metric_type_str: String = row
            .try_get(4)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let metric_type = metric_type_str.parse().unwrap_or(MetricType::RunDuration);

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

    fn metric_entry_from_postgres_row(
        &self, row: &sqlx::postgres::PgRow,
    ) -> DomainResult<MetricEntry> {
        let metric_type_str: String = row
            .try_get(4)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let metric_type = metric_type_str.parse().unwrap_or(MetricType::RunDuration);

        let metadata_str: Option<String> = row
            .try_get(6)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let metadata: Option<serde_json::Value> =
            metadata_str.and_then(|s| serde_json::from_str(&s).ok());

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
            timestamp: row
                .try_get(3)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            metric_type,
            value: row
                .try_get(5)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            metadata,
            created_at: row
                .try_get(7)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            run_hash,
        })
    }

    fn pipeline_metrics_stats_from_sqlite_row(
        &self, row: &sqlx::sqlite::SqliteRow,
    ) -> DomainResult<PipelineMetricsStats> {
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
    }

    fn pipeline_metrics_stats_from_postgres_row(
        &self, row: &sqlx::postgres::PgRow,
    ) -> DomainResult<PipelineMetricsStats> {
        let pipeline_id: String = row
            .try_get(0)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(PipelineMetricsStats {
            pipeline_id: pipeline_id.clone(),
            pipeline_name: pipeline_id,
            metrics_count: row
                .try_get(1)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            oldest_metric: row.try_get(2).ok(),
            newest_metric: row.try_get(3).ok(),
        })
    }

    pub async fn export_config(&self) -> DomainResult<MetricsConfigExport> {
        let global_config = self.get_global_config().await?;

        let pipeline_configs = match &self.pool {
            DatabasePool::Sqlite(p) => {
                let rows = sqlx::query(
                    "SELECT pipeline_id, enabled, retention_days, created_at, updated_at FROM metrics_config",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| self.metrics_config_from_sqlite_row(row))
                    .collect::<DomainResult<Vec<_>>>()?
            }
            DatabasePool::Postgres(p) => {
                let rows = sqlx::query(
                    "SELECT pipeline_id, enabled, retention_days::BIGINT, created_at, updated_at FROM metrics_config",
                )
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                rows.iter()
                    .map(|row| self.metrics_config_from_postgres_row(row))
                    .collect::<DomainResult<Vec<_>>>()?
            }
        };

        Ok(MetricsConfigExport {
            global_config,
            pipeline_configs,
        })
    }

    pub async fn import_config(&self, export: &MetricsConfigExport) -> DomainResult<()> {
        let start = std::time::Instant::now();

        self.update_global_config(
            export.global_config.enabled,
            export.global_config.default_retention_days,
        )
        .await?;

        if export.pipeline_configs.is_empty() {
            return Ok(());
        }

        let configs_count = export.pipeline_configs.len();

        const BATCH_SIZE: usize = 100;

        match &self.pool {
            DatabasePool::Sqlite(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for chunk in export.pipeline_configs.chunks(BATCH_SIZE) {
                    let values_clause = chunk
                        .iter()
                        .map(|_| "(?, ?, ?, datetime('now'), datetime('now'))")
                        .collect::<Vec<_>>()
                        .join(", ");

                    let sql = format!(
                        "INSERT OR REPLACE INTO metrics_config (pipeline_id, enabled, retention_days, created_at, updated_at) VALUES {}",
                        values_clause
                    );

                    let mut query = sqlx::query(&sql);
                    for config in chunk {
                        query = query
                            .bind(&config.pipeline_id)
                            .bind(config.enabled)
                            .bind(config.retention_days);
                    }

                    query
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for chunk in export.pipeline_configs.chunks(BATCH_SIZE) {
                    let mut param_idx = 0;
                    let values_clauses: Vec<String> = chunk
                        .iter()
                        .map(|_| {
                            let clause = format!(
                                "(${}, ${}, ${}, NOW(), NOW())",
                                param_idx + 1,
                                param_idx + 2,
                                param_idx + 3
                            );
                            param_idx += 3;
                            clause
                        })
                        .collect();

                    let sql = format!(
                        "INSERT INTO metrics_config (pipeline_id, enabled, retention_days, created_at, updated_at) VALUES {}
                         ON CONFLICT (pipeline_id) DO UPDATE SET enabled = EXCLUDED.enabled, retention_days = EXCLUDED.retention_days, updated_at = EXCLUDED.updated_at",
                        values_clauses.join(", ")
                    );

                    let mut query = sqlx::query(&sql);
                    for config in chunk {
                        query = query
                            .bind(&config.pipeline_id)
                            .bind(config.enabled)
                            .bind(config.retention_days);
                    }

                    query
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        let elapsed = start.elapsed();
        tracing::debug!(
            configs_count = configs_count,
            elapsed_ms = elapsed.as_millis(),
            "Imported metrics config (batch upsert)"
        );

        Ok(())
    }

    pub async fn check_processing_state_corruption(&self, pipeline_id: &str) -> DomainResult<bool> {
        let state = match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_as::<_, (i64,)>(
                    "SELECT last_processed_run_number FROM metrics_processing_state WHERE pipeline_id = ?",
                )
                .bind(pipeline_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_as::<_, (i64,)>(
                    "SELECT last_processed_run_number::BIGINT FROM metrics_processing_state WHERE pipeline_id = $1",
                )
                .bind(pipeline_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
        };

        if let Some((last_run,)) = state {
            if last_run > 0 {
                let count = match &self.pool {
                    DatabasePool::Sqlite(p) => sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM pipeline_metrics WHERE pipeline_id = ?",
                    )
                    .bind(pipeline_id)
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
                    DatabasePool::Postgres(p) => sqlx::query_as::<_, (i64,)>(
                        "SELECT COUNT(*) FROM pipeline_metrics WHERE pipeline_id = $1",
                    )
                    .bind(pipeline_id)
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
                };

                return Ok(count.0 == 0);
            }
        }
        Ok(false)
    }

    pub async fn reset_all_corrupted_states(&self) -> DomainResult<Vec<String>> {
        let corrupted = match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query_as::<_, (String,)>(
                r#"
                    SELECT DISTINCT mps.pipeline_id
                    FROM metrics_processing_state mps
                    WHERE mps.last_processed_run_number > 0
                    AND NOT EXISTS (
                        SELECT 1 FROM pipeline_metrics pm
                        WHERE pm.pipeline_id = mps.pipeline_id
                    )
                    "#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_as::<_, (String,)>(
                r#"
                    SELECT DISTINCT mps.pipeline_id
                    FROM metrics_processing_state mps
                    WHERE mps.last_processed_run_number > 0
                    AND NOT EXISTS (
                        SELECT 1 FROM pipeline_metrics pm
                        WHERE pm.pipeline_id = mps.pipeline_id
                    )
                    "#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
        };

        for (pipeline_id,) in &corrupted {
            self.reset_processing_state(pipeline_id).await?;
        }

        Ok(corrupted.into_iter().map(|(id,)| id).collect())
    }
}
