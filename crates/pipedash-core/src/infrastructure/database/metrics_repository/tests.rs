use chrono::{
    TimeZone,
    Utc,
};

use super::filters::append_filters;
use super::percentile::calculate_percentile;
use super::rows::parse_period_timestamp;
use crate::domain::{
    AggregationPeriod,
    MetricType,
    MetricsQuery,
};
use crate::infrastructure::database::bind::BindValue;
use crate::infrastructure::database::DatabasePool;

fn sqlite_pool() -> DatabasePool {
    DatabasePool::Sqlite(sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap())
}

#[tokio::test]
async fn filters_append_clauses_and_values_in_sql_order() {
    let query = MetricsQuery {
        pipeline_id: Some("p1".to_string()),
        metric_type: Some(MetricType::RunDuration),
        start_date: Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
        end_date: None,
        ..MetricsQuery::default()
    };
    let mut sql = String::from("SELECT 1 WHERE 1=1");

    let values = append_filters(&sqlite_pool(), &mut sql, &query, true);

    assert_eq!(
        sql,
        "SELECT 1 WHERE 1=1 AND pipeline_id = ? AND metric_type = ? AND timestamp >= ?"
    );
    assert!(matches!(
        values.as_slice(),
        [
            BindValue::Text("p1"),
            BindValue::Text(_),
            BindValue::Timestamp(_)
        ]
    ));
}

#[tokio::test]
async fn filters_skip_metric_type_when_not_requested() {
    let query = MetricsQuery {
        metric_type: Some(MetricType::RunDuration),
        ..MetricsQuery::default()
    };
    let mut sql = String::new();

    let values = append_filters(&sqlite_pool(), &mut sql, &query, false);

    assert!(sql.is_empty());
    assert!(values.is_empty());
}

#[test]
fn percentile_interpolates_between_neighbours() {
    assert_eq!(calculate_percentile(&[], 0.95), 0.0);
    assert_eq!(calculate_percentile(&[4.0], 0.95), 4.0);
    assert_eq!(calculate_percentile(&[1.0, 2.0, 3.0], 0.5), 2.0);
    assert!((calculate_percentile(&[1.0, 2.0, 3.0, 4.0], 0.5) - 2.5).abs() < f64::EPSILON);
}

#[test]
fn period_timestamps_accept_dates_and_datetimes() {
    let day = Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap();
    let hour = Utc.with_ymd_and_hms(2026, 3, 4, 5, 0, 0).unwrap();

    assert_eq!(
        parse_period_timestamp("2026-03-04", AggregationPeriod::Daily),
        day
    );
    assert_eq!(
        parse_period_timestamp("2026-03-04", AggregationPeriod::Weekly),
        day
    );
    assert_eq!(
        parse_period_timestamp("2026-03-04 05:00:00", AggregationPeriod::Hourly),
        hour
    );
}
