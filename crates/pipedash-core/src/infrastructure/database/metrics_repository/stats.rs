use chrono::{
    DateTime,
    Utc,
};

use super::{
    rows,
    MetricsRepository,
};
use crate::domain::{
    DomainResult,
    MetricsStats,
};
use crate::infrastructure::database::bind::fetch_rows;
use crate::infrastructure::database::DatabasePool;

const ESTIMATED_ROW_SIZE_BYTES: i64 = 250;
const OVERHEAD_MULTIPLIER: f64 = 1.2;

impl MetricsRepository {
    pub async fn calculate_storage_stats(&self) -> DomainResult<MetricsStats> {
        let total_metrics_count: i64 = on_pool!(&self.pool, p => sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_metrics")
                .fetch_one(p)
                .await
                .unwrap_or(0));

        let estimated_size_bytes =
            ((total_metrics_count * ESTIMATED_ROW_SIZE_BYTES) as f64 * OVERHEAD_MULTIPLIER) as i64;
        let estimated_size_mb = estimated_size_bytes as f64 / 1024.0 / 1024.0;

        let cleanup_sql = "SELECT last_cleanup_at FROM metrics_storage_info WHERE id = 1";
        let last_cleanup_at: Option<DateTime<Utc>> = match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, Option<String>>(cleanup_sql)
                .fetch_optional(p)
                .await
                .ok()
                .flatten()
                .flatten()
                .and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                }),
            DatabasePool::Postgres(p) => {
                sqlx::query_scalar::<_, Option<DateTime<Utc>>>(cleanup_sql)
                    .fetch_optional(p)
                    .await
                    .ok()
                    .flatten()
                    .flatten()
            }
        };

        let by_pipeline = fetch_rows(
            &self.pool,
            "SELECT pm.pipeline_id, COUNT(*) as count, MIN(pm.timestamp) as oldest, MAX(pm.timestamp) as newest
             FROM pipeline_metrics pm
             GROUP BY pm.pipeline_id",
            &[],
            rows::pipeline_stats_sqlite,
            rows::pipeline_stats_postgres,
        )
        .await?;

        Ok(MetricsStats {
            total_metrics_count,
            estimated_size_bytes,
            estimated_size_mb,
            last_cleanup_at,
            updated_at: Utc::now(),
            by_pipeline,
        })
    }

    pub async fn get_storage_stats(&self) -> DomainResult<MetricsStats> {
        self.calculate_storage_stats().await
    }
}
