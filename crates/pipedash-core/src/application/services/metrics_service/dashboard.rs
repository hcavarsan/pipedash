use chrono::{
    DateTime,
    Utc,
};

use super::MetricsService;
use crate::domain::{
    DateRange,
    DomainResult,
    MetricRangeOption,
    MetricsDashboard,
    MetricsDashboardQuery,
    MetricsQuery,
};

mod series;
#[cfg(test)]
mod tests;

const MS_PER_DAY: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

fn range_options(
    retention_days: i64, utc_offset_minutes: i32, now: DateTime<Utc>,
) -> Vec<MetricRangeOption> {
    DateRange::ALL
        .into_iter()
        .map(|value| {
            let span = now - value.cutoff(utc_offset_minutes, now);
            MetricRangeOption {
                value,
                enabled: span.num_milliseconds() as f64 / MS_PER_DAY <= retention_days as f64,
            }
        })
        .collect()
}

impl MetricsService {
    pub async fn dashboard(&self, query: MetricsDashboardQuery) -> DomainResult<MetricsDashboard> {
        self.dashboard_at(query, Utc::now()).await
    }

    async fn dashboard_at(
        &self, query: MetricsDashboardQuery, now: DateTime<Utc>,
    ) -> DomainResult<MetricsDashboard> {
        let aggregation_type = query
            .aggregation_type
            .unwrap_or_else(|| query.metric_type.default_aggregation());
        let data = self
            .query_aggregated_metrics(MetricsQuery {
                pipeline_id: query.pipeline_id.clone(),
                metric_type: Some(query.metric_type),
                start_date: Some(query.date_range.cutoff(query.utc_offset_minutes, now)),
                end_date: Some(now),
                aggregation_period: Some(query.aggregation_period),
                aggregation_type: Some(aggregation_type),
                limit: query.limit,
            })
            .await?;
        let data = series::fill_missing_periods(data);
        let observed: Vec<bool> = data
            .metrics
            .iter()
            .map(|metric| series::has_observation(metric, data.metric_type))
            .collect();
        let retention_days = match query.pipeline_id.as_deref() {
            Some(pipeline_id) => {
                self.get_effective_pipeline_config(pipeline_id)
                    .await?
                    .retention_days
            }
            None => self.get_global_config().await?.default_retention_days,
        };
        let ranges = range_options(retention_days, query.utc_offset_minutes, now);

        Ok(MetricsDashboard {
            stats: series::metric_stats(&data.metrics, &observed),
            table_indices: series::table_indices(&data.metrics, &observed, query.table_sort),
            widest_range: ranges
                .iter()
                .rfind(|range| range.enabled)
                .map_or(DateRange::Day, |range| range.value),
            ranges,
            observed,
            data,
            aggregation_type,
        })
    }
}
