use super::filters::append_filters;
use super::{
    rows,
    MetricsRepository,
};
use crate::domain::{
    DomainResult,
    MetricEntry,
    MetricsQuery,
};
use crate::infrastructure::database::bind::fetch_rows;

impl MetricsRepository {
    pub async fn query_metrics(&self, query: MetricsQuery) -> DomainResult<Vec<MetricEntry>> {
        let mut sql = format!(
            "SELECT id, pipeline_id, {}, timestamp, metric_type, value, metadata_json, created_at, run_hash FROM pipeline_metrics WHERE 1=1",
            self.pool.bigint("run_number")
        );
        let values = append_filters(&self.pool, &mut sql, &query, true);

        sql.push_str(" ORDER BY timestamp DESC");
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        fetch_rows(
            &self.pool,
            &sql,
            &values,
            rows::metric_entry_sqlite,
            rows::metric_entry_postgres,
        )
        .await
    }
}
