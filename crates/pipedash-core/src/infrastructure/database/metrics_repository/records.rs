use super::{
    metadata_json,
    MetricsRepository,
    RecordMetrics,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

const METRICS_LOCK_NAMESPACE: i32 = 0x6d65_7472;

fn delete_stale_sql(pool: &DatabasePool) -> String {
    format!(
        "DELETE FROM pipeline_metrics
         WHERE pipeline_id = {} AND run_number = {}
           AND (run_hash IS NULL OR run_hash <> {})",
        pool.placeholder(1),
        pool.placeholder(2),
        pool.placeholder(3)
    )
}

async fn refresh(
    pool: &DatabasePool, delete_sql: &str, pipeline_id: &str, records: &[RecordMetrics],
) -> DomainResult<usize> {
    let insert_sql = pool.pick(
        include_str!("../../../../sql/metrics/refresh_record_insert_sqlite.sql"),
        include_str!("../../../../sql/metrics/refresh_record_insert_postgres.sql"),
    );
    on_pool!(pool, p => {
        let mut affected_rows = 0;
        let mut tx = p.begin().await.map_err(db_err)?;

        if pool.is_postgres() {
            sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
                .bind(METRICS_LOCK_NAMESPACE)
                .bind(pipeline_id)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }

        for record in records {
            affected_rows += sqlx::query(sqlx::AssertSqlSafe(delete_sql))
                .bind(pipeline_id)
                .bind(record.run_number)
                .bind(&record.run_hash)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?
                .rows_affected() as usize;

            for metric in record.entries.iter().flatten() {
                let query = sqlx::query(insert_sql)
                    .bind(pipeline_id)
                    .bind(record.run_number)
                    .bind(metric.timestamp)
                    .bind(metric.metric_type.as_str())
                    .bind(metric.value)
                    .bind(metadata_json(metric))
                    .bind(metric.created_at)
                    .bind(&record.run_hash);
                let query = if pool.is_postgres() {
                    query
                } else {
                    query
                        .bind(pipeline_id)
                        .bind(record.run_number)
                        .bind(metric.metric_type.as_str())
                };
                affected_rows += query
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?
                    .rows_affected() as usize;
            }
        }

        tx.commit().await.map_err(db_err)?;
        Ok(affected_rows)
    })
}

impl MetricsRepository {
    pub async fn refresh_record_metrics(
        &self, pipeline_id: &str, records: &[RecordMetrics],
    ) -> DomainResult<usize> {
        if records.is_empty() {
            return Ok(0);
        }

        let delete_sql = delete_stale_sql(&self.pool);
        let affected_rows = refresh(&self.pool, &delete_sql, pipeline_id, records).await?;

        tracing::debug!(
            pipeline_id = %pipeline_id,
            records = records.len(),
            affected_rows,
            "Refreshed record metrics"
        );

        Ok(affected_rows)
    }
}
