use chrono::{
    DateTime,
    Utc,
};

use super::series::{
    advance_period,
    fill_missing_periods,
    has_observation,
    metric_stats,
    table_indices,
};
use crate::application::services::metrics_service::tests::{
    metric_entry,
    sqlite_service,
};
use crate::domain::{
    AggregatedMetric,
    AggregatedMetrics,
    AggregationPeriod,
    DateRange,
    MetricSortField,
    MetricTableSort,
    MetricType,
    MetricsDashboardQuery,
    SortDirection,
};

fn at(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

fn metric(timestamp: &str, value: f64, count: i64) -> AggregatedMetric {
    AggregatedMetric {
        timestamp: at(timestamp),
        value,
        count,
        min: (count > 0).then_some(value),
        max: (count > 0).then_some(value),
        avg: value,
    }
}

fn series(
    metric_type: MetricType, aggregation_period: AggregationPeriod, metrics: Vec<AggregatedMetric>,
) -> AggregatedMetrics {
    AggregatedMetrics {
        total_count: metrics.len(),
        metrics,
        metric_type,
        aggregation_period,
    }
}

fn observed(data: &AggregatedMetrics) -> Vec<bool> {
    data.metrics
        .iter()
        .map(|metric| has_observation(metric, data.metric_type))
        .collect()
}

#[test]
fn periods_advance_by_their_own_unit() {
    let start = at("2026-01-31T23:00:00Z");
    for (period, expected) in [
        (AggregationPeriod::Hourly, "2026-02-01T00:00:00Z"),
        (AggregationPeriod::Daily, "2026-02-01T23:00:00Z"),
        (AggregationPeriod::Weekly, "2026-02-07T23:00:00Z"),
    ] {
        assert_eq!(advance_period(start, period), Some(at(expected)));
    }
    assert_eq!(
        advance_period(at("2026-01-01T00:00:00Z"), AggregationPeriod::Monthly),
        Some(at("2026-02-01T00:00:00Z"))
    );
    assert_eq!(
        advance_period(at("2026-12-01T00:00:00Z"), AggregationPeriod::Monthly),
        Some(at("2027-01-01T00:00:00Z"))
    );
}

#[test]
fn gaps_between_observed_periods_are_zero_filled_chronologically() {
    let data = series(
        MetricType::RunDuration,
        AggregationPeriod::Hourly,
        vec![
            metric("2026-11-02T00:00:00Z", 60.0, 2),
            metric("2026-11-02T03:00:00Z", 30.0, 1),
        ],
    );

    let filled = fill_missing_periods(data);

    let timestamps: Vec<_> = filled.metrics.iter().map(|m| m.timestamp).collect();
    assert_eq!(
        timestamps,
        [
            at("2026-11-02T00:00:00Z"),
            at("2026-11-02T01:00:00Z"),
            at("2026-11-02T02:00:00Z"),
            at("2026-11-02T03:00:00Z"),
        ]
    );
    assert_eq!(filled.total_count, 4);
    let gap = &filled.metrics[1];
    assert_eq!(
        (gap.value, gap.count, gap.min, gap.max, gap.avg),
        (0.0, 0, None, None, 0.0)
    );
    assert_eq!(observed(&filled), [true, false, false, true]);
}

#[test]
fn monthly_gaps_step_by_calendar_month_and_edges_are_not_padded() {
    let data = series(
        MetricType::RunFrequency,
        AggregationPeriod::Monthly,
        vec![
            metric("2026-01-01T00:00:00Z", 4.0, 4),
            metric("2026-04-01T00:00:00Z", 2.0, 2),
        ],
    );

    let filled = fill_missing_periods(data);

    assert_eq!(
        filled
            .metrics
            .iter()
            .map(|m| m.timestamp)
            .collect::<Vec<_>>(),
        [
            at("2026-01-01T00:00:00Z"),
            at("2026-02-01T00:00:00Z"),
            at("2026-03-01T00:00:00Z"),
            at("2026-04-01T00:00:00Z"),
        ]
    );
    assert_eq!(observed(&filled), [true; 4]);

    let empty = fill_missing_periods(series(
        MetricType::RunDuration,
        AggregationPeriod::Daily,
        vec![],
    ));
    assert!(empty.metrics.is_empty());
    assert_eq!(empty.total_count, 0);
}

#[test]
fn stats_average_observed_periods_without_weighting_by_count() {
    let data = series(
        MetricType::RunDuration,
        AggregationPeriod::Daily,
        vec![
            metric("2026-11-01T00:00:00Z", 10.0, 100),
            metric("2026-11-02T00:00:00Z", 0.0, 0),
            metric("2026-11-03T00:00:00Z", 30.0, 1),
            metric("2026-11-04T00:00:00Z", 0.0, 0),
        ],
    );

    let stats = metric_stats(&data.metrics, &observed(&data));

    assert_eq!(stats.average, Some(20.0));
    assert_eq!(stats.latest, Some(30.0));
    assert_eq!(stats.max, Some(30.0));
    assert_eq!((stats.observed, stats.total), (2, 4));
}

#[test]
fn stats_without_observations_are_empty_and_frequency_counts_quiet_periods() {
    let quiet = series(
        MetricType::RunDuration,
        AggregationPeriod::Daily,
        vec![metric("2026-11-01T00:00:00Z", 0.0, 0)],
    );
    let stats = metric_stats(&quiet.metrics, &observed(&quiet));
    assert_eq!(
        (
            stats.average,
            stats.latest,
            stats.max,
            stats.observed,
            stats.total
        ),
        (None, None, None, 0, 1)
    );

    let frequency = series(
        MetricType::RunFrequency,
        AggregationPeriod::Daily,
        vec![
            metric("2026-11-01T00:00:00Z", 3.0, 3),
            metric("2026-11-02T00:00:00Z", 0.0, 0),
        ],
    );
    let stats = metric_stats(&frequency.metrics, &observed(&frequency));
    assert_eq!(stats.average, Some(1.5));
    assert_eq!(stats.latest, Some(0.0));
    assert_eq!((stats.observed, stats.total), (2, 2));
}

#[test]
fn table_order_is_stable_and_puts_missing_values_last_in_both_directions() {
    let data = series(
        MetricType::RunDuration,
        AggregationPeriod::Daily,
        vec![
            metric("2026-11-01T00:00:00Z", 20.0, 1),
            metric("2026-11-02T00:00:00Z", 0.0, 0),
            metric("2026-11-03T00:00:00Z", 10.0, 1),
            metric("2026-11-04T00:00:00Z", 20.0, 1),
        ],
    );
    let observed = observed(&data);
    let sort = |field, direction| Some(MetricTableSort { field, direction });

    assert_eq!(table_indices(&data.metrics, &observed, None), [0, 1, 2, 3]);
    assert_eq!(
        table_indices(
            &data.metrics,
            &observed,
            sort(MetricSortField::Value, SortDirection::Asc)
        ),
        [2, 0, 3, 1]
    );
    assert_eq!(
        table_indices(
            &data.metrics,
            &observed,
            sort(MetricSortField::Value, SortDirection::Desc)
        ),
        [0, 3, 2, 1]
    );
    assert_eq!(
        table_indices(
            &data.metrics,
            &observed,
            sort(MetricSortField::Min, SortDirection::Desc)
        ),
        [0, 3, 2, 1]
    );
    assert_eq!(
        table_indices(
            &data.metrics,
            &observed,
            sort(MetricSortField::Timestamp, SortDirection::Desc)
        ),
        [3, 2, 1, 0]
    );
    assert_eq!(
        table_indices(
            &data.metrics,
            &observed,
            sort(MetricSortField::Count, SortDirection::Asc)
        ),
        [1, 0, 2, 3]
    );
}

#[test]
fn ranges_beyond_retention_are_disabled_and_widest_is_the_last_enabled() {
    let now = at("2026-11-06T12:00:00Z");
    let enabled = |retention| -> Vec<bool> {
        super::range_options(retention, 0, now)
            .iter()
            .map(|range| range.enabled)
            .collect()
    };

    assert_eq!(enabled(7), [true, true, true, false, false, false]);
    assert_eq!(enabled(90), [true; 6]);
    assert_eq!(enabled(1), [true, true, false, false, false, false]);
}

fn query(pipeline_id: &str) -> MetricsDashboardQuery {
    MetricsDashboardQuery {
        pipeline_id: Some(pipeline_id.to_string()),
        metric_type: MetricType::RunDuration,
        aggregation_period: AggregationPeriod::Daily,
        aggregation_type: None,
        date_range: DateRange::Week,
        utc_offset_minutes: 0,
        limit: None,
        table_sort: None,
    }
}

#[tokio::test]
async fn dashboard_fills_gaps_and_reports_observations_stats_and_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let service = sqlite_service(&dir).await;
    let pipeline_id = "stub__1";
    service
        .update_pipeline_config(pipeline_id, true, 7)
        .await
        .unwrap();
    service
        .repository()
        .insert_metrics_batch(vec![
            metric_entry(
                pipeline_id,
                1,
                MetricType::RunDuration,
                "2026-11-02T01:00:00Z",
                60.0,
            ),
            metric_entry(
                pipeline_id,
                2,
                MetricType::RunDuration,
                "2026-11-02T02:00:00Z",
                120.0,
            ),
            metric_entry(
                pipeline_id,
                3,
                MetricType::RunDuration,
                "2026-11-05T01:00:00Z",
                30.0,
            ),
            metric_entry(
                pipeline_id,
                4,
                MetricType::RunDuration,
                "2026-10-01T01:00:00Z",
                999.0,
            ),
        ])
        .await
        .unwrap();
    let mut request = query(pipeline_id);
    request.table_sort = Some(MetricTableSort {
        field: MetricSortField::Value,
        direction: SortDirection::Desc,
    });

    let dashboard = service
        .dashboard_at(request, at("2026-11-06T12:00:00Z"))
        .await
        .unwrap();

    let timestamps: Vec<_> = dashboard.data.metrics.iter().map(|m| m.timestamp).collect();
    assert_eq!(
        timestamps,
        [
            at("2026-11-02T00:00:00Z"),
            at("2026-11-03T00:00:00Z"),
            at("2026-11-04T00:00:00Z"),
            at("2026-11-05T00:00:00Z"),
        ]
    );
    assert_eq!(dashboard.data.total_count, 4);
    assert_eq!(dashboard.observed, [true, false, false, true]);
    assert_eq!(dashboard.table_indices, [0, 3, 1, 2]);
    assert_eq!(dashboard.stats.average, Some(60.0));
    assert_eq!(dashboard.stats.latest, Some(30.0));
    assert_eq!(dashboard.stats.max, Some(90.0));
    assert_eq!((dashboard.stats.observed, dashboard.stats.total), (2, 4));
    assert_eq!(
        dashboard.aggregation_type,
        crate::domain::AggregationType::Avg
    );
    assert_eq!(dashboard.widest_range, DateRange::Week);
    assert_eq!(
        dashboard
            .ranges
            .iter()
            .map(|r| r.enabled)
            .collect::<Vec<_>>(),
        [true, true, true, false, false, false]
    );
}

#[tokio::test]
async fn dashboard_without_pipeline_uses_global_retention_and_frequency_sums() {
    let dir = tempfile::tempdir().unwrap();
    let service = sqlite_service(&dir).await;
    service.update_global_config(true, 30).await.unwrap();
    service
        .repository()
        .insert_metrics_batch(vec![
            metric_entry(
                "stub__1",
                1,
                MetricType::RunFrequency,
                "2026-11-05T01:00:00Z",
                1.0,
            ),
            metric_entry(
                "stub__1",
                2,
                MetricType::RunFrequency,
                "2026-11-05T02:00:00Z",
                1.0,
            ),
        ])
        .await
        .unwrap();
    let mut request = query("unused");
    request.pipeline_id = None;
    request.metric_type = MetricType::RunFrequency;

    let dashboard = service
        .dashboard_at(request, at("2026-11-06T12:00:00Z"))
        .await
        .unwrap();

    assert_eq!(
        dashboard.aggregation_type,
        crate::domain::AggregationType::Sum
    );
    assert_eq!(dashboard.data.metrics[0].value, 2.0);
    assert_eq!(dashboard.widest_range, DateRange::Month);
}
