use std::time::Instant;

use super::{
    metadata_json,
    MetricsRepository,
};
use crate::domain::{
    DomainResult,
    MetricEntry,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

const BATCH_SIZE: usize = 100;
const COLUMNS: &str =
    "pipeline_metrics (pipeline_id, run_number, timestamp, metric_type, value, metadata_json, created_at, run_hash)";

fn insert_sql(pool: &DatabasePool, rows: usize) -> String {
    let values = (0..rows)
        .map(|row| {
            let placeholders = (1..=8)
                .map(|column| pool.placeholder(row * 8 + column))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({placeholders})")
        })
        .collect::<Vec<_>>()
        .join(", ");

    match pool {
        DatabasePool::Sqlite(_) => format!("INSERT OR IGNORE INTO {COLUMNS} VALUES {values}"),
        DatabasePool::Postgres(_) => {
            format!("INSERT INTO {COLUMNS} VALUES {values} ON CONFLICT DO NOTHING")
        }
    }
}

async fn insert_batch(pool: &DatabasePool, metrics: &[MetricEntry]) -> DomainResult<usize> {
    on_pool!(pool, p => {
        let mut tx = p.begin().await.map_err(db_err)?;
        let mut inserted = 0;

        for chunk in metrics.chunks(BATCH_SIZE) {
            let mut query = sqlx::query(sqlx::AssertSqlSafe(insert_sql(pool, chunk.len())));
            for metric in chunk {
                query = query
                    .bind(&metric.pipeline_id)
                    .bind(metric.run_number)
                    .bind(metric.timestamp)
                    .bind(metric.metric_type.as_str())
                    .bind(metric.value)
                    .bind(metadata_json(metric))
                    .bind(metric.created_at)
                    .bind(&metric.run_hash);
            }
            inserted += query
                .execute(&mut *tx)
                .await
                .map_err(db_err)?
                .rows_affected() as usize;
        }

        tx.commit().await.map_err(db_err)?;
        Ok(inserted)
    })
}

impl MetricsRepository {
    pub async fn insert_metrics_batch(&self, metrics: Vec<MetricEntry>) -> DomainResult<usize> {
        if metrics.is_empty() {
            return Ok(0);
        }

        let start = Instant::now();
        let inserted = insert_batch(&self.pool, &metrics).await?;

        let skipped = metrics.len() - inserted;
        if skipped > 0 {
            tracing::debug!(skipped = skipped, "Skipped duplicate metrics");
        }

        tracing::debug!(
            metrics_count = metrics.len(),
            inserted = inserted,
            skipped = skipped,
            elapsed_ms = start.elapsed().as_millis(),
            "Inserted metrics batch"
        );

        Ok(inserted)
    }
}
