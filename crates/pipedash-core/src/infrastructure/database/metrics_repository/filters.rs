use crate::domain::MetricsQuery;
use crate::infrastructure::database::bind::BindValue;
use crate::infrastructure::database::DatabasePool;

pub(super) fn append_filters<'a>(
    pool: &DatabasePool, sql: &mut String, query: &'a MetricsQuery, include_metric_type: bool,
) -> Vec<BindValue<'a>> {
    let mut values = Vec::new();
    let mut push = |sql: &mut String, clause: &str, value: BindValue<'a>| {
        values.push(value);
        sql.push_str(&format!(" AND {clause} {}", pool.placeholder(values.len())));
    };

    if let Some(pipeline_id) = &query.pipeline_id {
        push(sql, "pipeline_id =", BindValue::Text(pipeline_id));
    }
    if let Some(metric_type) = query.metric_type.filter(|_| include_metric_type) {
        push(sql, "metric_type =", BindValue::Text(metric_type.as_str()));
    }
    if let Some(start_date) = query.start_date {
        push(sql, "timestamp >=", BindValue::Timestamp(start_date));
    }
    if let Some(end_date) = query.end_date {
        push(sql, "timestamp <=", BindValue::Timestamp(end_date));
    }

    values
}
