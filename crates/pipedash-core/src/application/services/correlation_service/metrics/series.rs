use std::collections::{
    BTreeMap,
    BTreeSet,
};

use chrono::{
    DateTime,
    Datelike,
    DurationRound,
    TimeDelta,
    TimeZone,
    Utc,
};

use super::context::Ctx;
use crate::application::services::correlation_service::copy;
use crate::domain::application::metrics::{
    Coverage,
    MetricId,
    MetricPoint,
    MetricResult,
    MetricUnit,
};
use crate::domain::metrics::AggregationPeriod;
use crate::infrastructure::database::metrics_repository::percentile::calculate_percentile;

pub(super) struct Sample {
    pub(super) at: DateTime<Utc>,
    pub(super) value: f64,
    pub(super) change_id: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Fold {
    Mean,
    Sum,
}

pub(super) struct Spec {
    pub(super) id: MetricId,
    pub(super) unit: MetricUnit,
    pub(super) fold: Fold,
    pub(super) spread: bool,
    pub(super) windowed: bool,
}

impl Spec {
    pub(super) fn seconds(id: MetricId) -> Self {
        Self {
            id,
            unit: MetricUnit::Seconds,
            fold: Fold::Mean,
            spread: false,
            windowed: false,
        }
    }

    pub(super) fn percent(id: MetricId) -> Self {
        Self {
            id,
            unit: MetricUnit::Percent,
            fold: Fold::Mean,
            spread: false,
            windowed: false,
        }
    }

    pub(super) fn spread(self) -> Self {
        Self {
            spread: true,
            ..self
        }
    }

    pub(super) fn windowed(self) -> Self {
        Self {
            windowed: true,
            ..self
        }
    }
}

pub(super) fn bucket_start(at: DateTime<Utc>, period: AggregationPeriod) -> DateTime<Utc> {
    let day = at.duration_trunc(TimeDelta::days(1)).unwrap_or(at);
    match period {
        AggregationPeriod::Hourly => at.duration_trunc(TimeDelta::hours(1)).unwrap_or(at),
        AggregationPeriod::Daily => day,
        AggregationPeriod::Weekly => {
            day - TimeDelta::days(i64::from(at.weekday().num_days_from_monday()))
        }
        AggregationPeriod::Monthly => Utc
            .with_ymd_and_hms(at.year(), at.month(), 1, 0, 0, 0)
            .single()
            .unwrap_or(day),
    }
}

fn fold(values: &[f64], how: Fold) -> f64 {
    let sum: f64 = values.iter().sum();
    match how {
        Fold::Sum => sum,
        Fold::Mean => sum / values.len() as f64,
    }
}

fn sorted(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values
}

pub(super) fn summarize(
    ctx: &Ctx, spec: Spec, samples: Vec<Sample>, reason: String,
) -> MetricResult {
    let samples: Vec<Sample> = samples
        .into_iter()
        .filter(|sample| ctx.in_range(sample.at))
        .collect();
    let counted = samples
        .iter()
        .map(|sample| sample.change_id)
        .collect::<BTreeSet<_>>()
        .len();
    let excluded = ctx.scope_total().saturating_sub(counted);
    let coverage = Coverage {
        counted,
        excluded,
        since: if spec.windowed {
            ctx.window_since()
        } else {
            ctx.since()
        },
        label: copy::coverage(counted, excluded),
    };
    let mut result = MetricResult {
        id: spec.id,
        label: copy::metric_label(spec.id),
        unit: spec.unit,
        value: None,
        p50: None,
        p95: None,
        unavailable_reason: Some(reason),
        coverage,
        series: Vec::new(),
    };
    if samples.is_empty() {
        return result;
    }
    let values: Vec<f64> = samples.iter().map(|sample| sample.value).collect();
    result.value = Some(fold(&values, spec.fold));
    result.unavailable_reason = None;
    if spec.spread {
        let ordered = sorted(values);
        result.p50 = Some(calculate_percentile(&ordered, 0.5));
        result.p95 = Some(calculate_percentile(&ordered, 0.95));
    }
    let mut buckets: BTreeMap<DateTime<Utc>, Vec<f64>> = BTreeMap::new();
    for sample in &samples {
        buckets
            .entry(bucket_start(sample.at, ctx.period))
            .or_default()
            .push(sample.value);
    }
    result.series = buckets
        .into_iter()
        .map(|(timestamp, values)| MetricPoint {
            timestamp,
            value: fold(&values, spec.fold),
            count: values.len(),
        })
        .collect();
    result
}
