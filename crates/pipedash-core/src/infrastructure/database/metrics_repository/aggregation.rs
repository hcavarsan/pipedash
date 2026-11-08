use super::filters::append_filters;
use super::{
    rows,
    MetricsRepository,
};
use crate::domain::{
    AggregatedMetrics,
    AggregationPeriod,
    AggregationType,
    DomainError,
    DomainResult,
    MetricsQuery,
};
use crate::infrastructure::database::bind::fetch_rows;
use crate::infrastructure::database::DatabasePool;

pub(super) fn period_select(pool: &DatabasePool, period: AggregationPeriod) -> &'static str {
    match (pool, period) {
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
        (DatabasePool::Postgres(_), AggregationPeriod::Hourly) => "date_trunc('hour', timestamp)",
        (DatabasePool::Postgres(_), AggregationPeriod::Daily) => "date_trunc('day', timestamp)",
        (DatabasePool::Postgres(_), AggregationPeriod::Weekly) => "date_trunc('week', timestamp)",
        (DatabasePool::Postgres(_), AggregationPeriod::Monthly) => "date_trunc('month', timestamp)",
    }
}

impl MetricsRepository {
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

        let aggregation_select = match aggregation_type {
            AggregationType::Avg => "AVG(value)",
            AggregationType::Sum => "SUM(value)",
            AggregationType::Min => "MIN(value)",
            AggregationType::Max => "MAX(value)",
            AggregationType::P95 | AggregationType::P99 => {
                let metrics = self
                    .calculate_percentile_metrics(&query, aggregation_period, aggregation_type)
                    .await?;
                return Ok(aggregated(metrics, metric_type, aggregation_period));
            }
        };

        let mut sql = format!(
            "SELECT
                {} as period,
                {aggregation_select} as agg_value,
                COUNT(*) as count,
                MIN(value) as min_value,
                MAX(value) as max_value,
                AVG(value) as avg_value
             FROM pipeline_metrics
             WHERE 1=1",
            period_select(&self.pool, aggregation_period)
        );
        let values = append_filters(&self.pool, &mut sql, &query, true);

        sql.push_str(" GROUP BY period ORDER BY period ASC");
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let metrics = fetch_rows(
            &self.pool,
            &sql,
            &values,
            |row| rows::aggregated_sqlite(row, aggregation_period),
            rows::aggregated_postgres,
        )
        .await?;

        Ok(aggregated(metrics, metric_type, aggregation_period))
    }
}

fn aggregated(
    metrics: Vec<crate::domain::AggregatedMetric>, metric_type: crate::domain::MetricType,
    aggregation_period: AggregationPeriod,
) -> AggregatedMetrics {
    AggregatedMetrics {
        total_count: metrics.len(),
        metrics,
        metric_type,
        aggregation_period,
    }
}
