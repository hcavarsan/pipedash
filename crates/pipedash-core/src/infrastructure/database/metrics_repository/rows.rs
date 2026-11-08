use chrono::{
    DateTime,
    Utc,
};
use sqlx::postgres::PgRow;
use sqlx::sqlite::SqliteRow;
use sqlx::Row as SqlxRow;

use crate::domain::{
    AggregatedMetric,
    AggregationPeriod,
    DomainResult,
    GlobalMetricsConfig,
    MetricEntry,
    MetricType,
    MetricsConfig,
    PipelineMetricsStats,
};
use crate::infrastructure::database::pool::db_err;

fn parse_rfc3339(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn optional_rfc3339(value: Option<String>) -> Option<DateTime<Utc>> {
    value.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    })
}

pub(super) fn global_config_sqlite(row: &SqliteRow) -> DomainResult<GlobalMetricsConfig> {
    let enabled: i64 = row.try_get(0).map_err(db_err)?;
    let updated_at: String = row.try_get(2).map_err(db_err)?;

    Ok(GlobalMetricsConfig {
        enabled: enabled != 0,
        default_retention_days: row.try_get(1).map_err(db_err)?,
        updated_at: parse_rfc3339(&updated_at),
    })
}

pub(super) fn global_config_postgres(row: &PgRow) -> DomainResult<GlobalMetricsConfig> {
    Ok(GlobalMetricsConfig {
        enabled: row.try_get(0).map_err(db_err)?,
        default_retention_days: row.try_get(1).map_err(db_err)?,
        updated_at: row.try_get(2).map_err(db_err)?,
    })
}

pub(super) fn metrics_config_sqlite(row: &SqliteRow) -> DomainResult<MetricsConfig> {
    let enabled: i64 = row.try_get(1).map_err(db_err)?;
    let created_at: String = row.try_get(3).map_err(db_err)?;
    let updated_at: String = row.try_get(4).map_err(db_err)?;

    Ok(MetricsConfig {
        pipeline_id: row.try_get(0).map_err(db_err)?,
        enabled: enabled != 0,
        retention_days: row.try_get(2).map_err(db_err)?,
        created_at: parse_rfc3339(&created_at),
        updated_at: parse_rfc3339(&updated_at),
    })
}

pub(super) fn metrics_config_postgres(row: &PgRow) -> DomainResult<MetricsConfig> {
    Ok(MetricsConfig {
        pipeline_id: row.try_get(0).map_err(db_err)?,
        enabled: row.try_get(1).map_err(db_err)?,
        retention_days: row.try_get(2).map_err(db_err)?,
        created_at: row.try_get(3).map_err(db_err)?,
        updated_at: row.try_get(4).map_err(db_err)?,
    })
}

fn metric_entry<'r, R>(
    row: &'r R, timestamp: DateTime<Utc>, created_at: DateTime<Utc>,
) -> DomainResult<MetricEntry>
where
    R: SqlxRow,
    usize: sqlx::ColumnIndex<R>,
    i64: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    f64: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    String: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    Option<String>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
{
    let metric_type: String = row.try_get(4).map_err(db_err)?;
    let metadata: Option<String> = row.try_get(6).map_err(db_err)?;

    Ok(MetricEntry {
        id: row.try_get(0).map_err(db_err)?,
        pipeline_id: row.try_get(1).map_err(db_err)?,
        run_number: row.try_get(2).map_err(db_err)?,
        timestamp,
        metric_type: metric_type.parse().unwrap_or(MetricType::RunDuration),
        value: row.try_get(5).map_err(db_err)?,
        metadata: metadata.and_then(|s| serde_json::from_str(&s).ok()),
        created_at,
        run_hash: row.try_get(8).ok(),
    })
}

pub(super) fn metric_entry_sqlite(row: &SqliteRow) -> DomainResult<MetricEntry> {
    let timestamp: String = row.try_get(3).map_err(db_err)?;
    let created_at: String = row.try_get(7).map_err(db_err)?;

    metric_entry(row, parse_rfc3339(&timestamp), parse_rfc3339(&created_at))
}

pub(super) fn metric_entry_postgres(row: &PgRow) -> DomainResult<MetricEntry> {
    metric_entry(
        row,
        row.try_get(3).map_err(db_err)?,
        row.try_get(7).map_err(db_err)?,
    )
}

fn pipeline_stats(
    pipeline_id: String, metrics_count: i64, oldest_metric: Option<DateTime<Utc>>,
    newest_metric: Option<DateTime<Utc>>,
) -> PipelineMetricsStats {
    PipelineMetricsStats {
        pipeline_name: pipeline_id.clone(),
        pipeline_id,
        metrics_count,
        oldest_metric,
        newest_metric,
    }
}

pub(super) fn pipeline_stats_sqlite(row: &SqliteRow) -> DomainResult<PipelineMetricsStats> {
    Ok(pipeline_stats(
        row.try_get(0).map_err(db_err)?,
        row.try_get(1).map_err(db_err)?,
        optional_rfc3339(row.try_get(2).map_err(db_err)?),
        optional_rfc3339(row.try_get(3).map_err(db_err)?),
    ))
}

pub(super) fn pipeline_stats_postgres(row: &PgRow) -> DomainResult<PipelineMetricsStats> {
    Ok(pipeline_stats(
        row.try_get(0).map_err(db_err)?,
        row.try_get(1).map_err(db_err)?,
        row.try_get(2).ok(),
        row.try_get(3).ok(),
    ))
}

pub(super) fn aggregated_metric(
    timestamp: DateTime<Utc>, value: f64, count: i64, min: Option<f64>, max: Option<f64>, avg: f64,
) -> AggregatedMetric {
    AggregatedMetric {
        timestamp,
        value,
        count,
        min,
        max,
        avg,
    }
}

fn aggregated_row<'r, R>(row: &'r R, timestamp: DateTime<Utc>) -> DomainResult<AggregatedMetric>
where
    R: SqlxRow,
    usize: sqlx::ColumnIndex<R>,
    f64: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    i64: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
{
    Ok(aggregated_metric(
        timestamp,
        row.try_get::<f64, _>(1).map_err(db_err)?,
        row.try_get(2).map_err(db_err)?,
        row.try_get(3).ok(),
        row.try_get(4).ok(),
        row.try_get(5).map_err(db_err)?,
    ))
}

pub(super) fn aggregated_sqlite(
    row: &SqliteRow, period: AggregationPeriod,
) -> DomainResult<AggregatedMetric> {
    let period_str: String = row.try_get(0).map_err(db_err)?;

    aggregated_row(row, parse_period_timestamp(&period_str, period))
}

pub(super) fn aggregated_postgres(row: &PgRow) -> DomainResult<AggregatedMetric> {
    aggregated_row(row, row.try_get(0).map_err(db_err)?)
}

pub(super) fn period_value_sqlite(row: &SqliteRow) -> DomainResult<(String, f64)> {
    Ok((
        row.try_get(0).map_err(db_err)?,
        row.try_get(1).map_err(db_err)?,
    ))
}

pub(super) fn period_value_postgres(row: &PgRow) -> DomainResult<(String, f64)> {
    let period: DateTime<Utc> = row.try_get(0).map_err(db_err)?;

    Ok((period.to_rfc3339(), row.try_get(1).map_err(db_err)?))
}

pub(super) fn parse_period_timestamp(
    period_str: &str, aggregation_period: AggregationPeriod,
) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(period_str) {
        return dt.with_timezone(&Utc);
    }

    let datetime_str =
        if aggregation_period != AggregationPeriod::Weekly && period_str.contains(' ') {
            format!("{} +0000", period_str)
        } else {
            format!("{} 00:00:00 +0000", period_str)
        };

    DateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M:%S %z")
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
