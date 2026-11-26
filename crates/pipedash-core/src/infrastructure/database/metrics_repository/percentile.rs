use std::collections::BTreeMap;

use super::aggregation::period_select;
use super::filters::append_filters;
use super::rows::{
    self,
    parse_period_timestamp,
};
use super::MetricsRepository;
use crate::domain::{
    AggregatedMetric,
    AggregationPeriod,
    AggregationType,
    DomainResult,
    MetricsQuery,
};
use crate::infrastructure::database::bind::fetch_rows;

pub(crate) fn calculate_percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    match sorted_values {
        [] => return 0.0,
        [only] => return *only,
        _ => {}
    }

    let position = percentile * (sorted_values.len() - 1) as f64;
    let lower_index = position.floor() as usize;
    let upper_index = position.ceil() as usize;

    if lower_index == upper_index {
        return sorted_values[lower_index];
    }

    let lower_value = sorted_values[lower_index];
    let upper_value = sorted_values[upper_index];
    lower_value + (upper_value - lower_value) * (position - lower_index as f64)
}

fn period_metric(
    period: &str, mut values: Vec<f64>, percentile: f64, aggregation_period: AggregationPeriod,
) -> Option<AggregatedMetric> {
    if values.is_empty() {
        return None;
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = values.len();

    Some(rows::aggregated_metric(
        parse_period_timestamp(period, aggregation_period),
        calculate_percentile(&values, percentile),
        count as i64,
        values.first().copied(),
        values.last().copied(),
        values.iter().sum::<f64>() / count as f64,
    ))
}

impl MetricsRepository {
    pub(super) async fn calculate_percentile_metrics(
        &self, query: &MetricsQuery, aggregation_period: AggregationPeriod,
        aggregation_type: AggregationType,
    ) -> DomainResult<Vec<AggregatedMetric>> {
        let percentile = if matches!(aggregation_type, AggregationType::P95) {
            0.95
        } else {
            0.99
        };

        let mut sql = format!(
            "SELECT {} as period, value FROM pipeline_metrics WHERE 1=1",
            period_select(&self.pool, aggregation_period)
        );
        let values = append_filters(&self.pool, &mut sql, query, true);
        sql.push_str(" ORDER BY period, value");

        let data = fetch_rows(
            &self.pool,
            &sql,
            &values,
            rows::period_value_sqlite,
            rows::period_value_postgres,
        )
        .await?;

        let mut period_values: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for (period, value) in data.into_iter().filter(|(_, value)| value.is_finite()) {
            period_values.entry(period).or_default().push(value);
        }

        let mut metrics: Vec<AggregatedMetric> = period_values
            .into_iter()
            .filter_map(|(period, values)| {
                period_metric(&period, values, percentile, aggregation_period)
            })
            .collect();
        metrics.sort_by_key(|metric| metric.timestamp);

        if let Some(limit) = query.limit {
            metrics.truncate(limit);
        }

        Ok(metrics)
    }
}
