use std::cmp::Ordering;

use chrono::{
    DateTime,
    Duration,
    Months,
    Utc,
};

use crate::domain::{
    AggregatedMetric,
    AggregatedMetrics,
    AggregationPeriod,
    MetricSortField,
    MetricStats,
    MetricTableSort,
    MetricType,
    SortDirection,
};

pub(super) fn advance_period(
    date: DateTime<Utc>, period: AggregationPeriod,
) -> Option<DateTime<Utc>> {
    match period {
        AggregationPeriod::Hourly => date.checked_add_signed(Duration::hours(1)),
        AggregationPeriod::Daily => date.checked_add_signed(Duration::days(1)),
        AggregationPeriod::Weekly => date.checked_add_signed(Duration::days(7)),
        AggregationPeriod::Monthly => date.checked_add_months(Months::new(1)),
    }
}

pub(super) fn fill_missing_periods(data: AggregatedMetrics) -> AggregatedMetrics {
    let period = data.aggregation_period;
    let mut filled = Vec::with_capacity(data.metrics.len());
    let mut metrics = data.metrics.into_iter().peekable();

    while let Some(metric) = metrics.next() {
        let start = metric.timestamp;
        filled.push(metric);
        let Some(end) = metrics.peek().map(|following| following.timestamp) else {
            break;
        };
        let mut gap = advance_period(start, period);
        while let Some(timestamp) = gap.filter(|timestamp| *timestamp < end) {
            filled.push(AggregatedMetric {
                timestamp,
                value: 0.0,
                count: 0,
                min: None,
                max: None,
                avg: 0.0,
            });
            gap = advance_period(timestamp, period);
        }
    }

    AggregatedMetrics {
        total_count: filled.len(),
        metrics: filled,
        ..data
    }
}

pub(super) fn has_observation(metric: &AggregatedMetric, metric_type: MetricType) -> bool {
    metric_type == MetricType::RunFrequency || metric.count > 0
}

pub(super) fn metric_stats(metrics: &[AggregatedMetric], observed: &[bool]) -> MetricStats {
    let values: Vec<f64> = metrics
        .iter()
        .zip(observed)
        .filter(|(_, observed)| **observed)
        .map(|(metric, _)| metric.value)
        .collect();

    MetricStats {
        average: (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64),
        latest: values.last().copied(),
        max: values.iter().copied().reduce(f64::max),
        observed: values.len(),
        total: metrics.len(),
    }
}

fn sort_value(metric: &AggregatedMetric, observed: bool, field: MetricSortField) -> Option<f64> {
    match field {
        MetricSortField::Timestamp => Some(metric.timestamp.timestamp_millis() as f64),
        MetricSortField::Value => observed.then_some(metric.value),
        MetricSortField::Count => Some(metric.count as f64),
        MetricSortField::Min => metric.min,
        MetricSortField::Max => metric.max,
    }
}

pub(super) fn table_indices(
    metrics: &[AggregatedMetric], observed: &[bool], sort: Option<MetricTableSort>,
) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..metrics.len()).collect();
    let Some(sort) = sort else {
        return indices;
    };
    let keys: Vec<Option<f64>> = metrics
        .iter()
        .zip(observed)
        .map(|(metric, observed)| sort_value(metric, *observed, sort.field))
        .collect();

    indices.sort_by(|&a, &b| match (keys[a], keys[b]) {
        (Some(a), Some(b)) => match sort.direction {
            SortDirection::Asc => a.total_cmp(&b),
            SortDirection::Desc => b.total_cmp(&a),
        },
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
    });
    indices
}
