use std::collections::HashSet;

use chrono::Utc;

use super::MetricsRepository;
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::{
    execute_bound,
    BindValue,
};
use crate::infrastructure::database::{
    pool::db_err,
    DatabasePool,
};

impl MetricsRepository {
    pub async fn list_pipeline_ids(&self) -> DomainResult<Vec<String>> {
        let sql = "SELECT pipeline_id FROM pipeline_metrics
                   UNION SELECT pipeline_id FROM metrics_processing_state";
        on_pool!(&self.pool, p => sqlx::query_scalar::<_, String>(sql).fetch_all(p).await)
            .map_err(db_err)
    }

    pub async fn get_last_processed_run(&self, pipeline_id: &str) -> DomainResult<Option<i64>> {
        let sql = format!(
            "SELECT {} FROM metrics_processing_state WHERE pipeline_id = {}",
            self.pool.bigint("last_processed_run_number"),
            self.pool.placeholder(1)
        );
        on_pool!(&self.pool, p => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .bind(pipeline_id)
            .fetch_optional(p)
            .await)
        .map_err(db_err)
    }

    pub async fn get_stored_run_numbers_after(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<HashSet<i64>> {
        let sql = format!(
            "SELECT DISTINCT {} FROM pipeline_metrics WHERE pipeline_id = {} AND run_number > {}",
            self.pool.bigint("run_number"),
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let run_numbers =
            on_pool!(&self.pool, p => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .bind(pipeline_id)
            .bind(run_number)
            .fetch_all(p)
            .await)
            .map_err(db_err)?;

        Ok(run_numbers.into_iter().collect())
    }

    pub async fn update_last_processed_run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO metrics_processing_state (pipeline_id, last_processed_run_number, last_processed_at, updated_at)
             VALUES ({})
             ON CONFLICT(pipeline_id) DO UPDATE SET
                 last_processed_run_number = excluded.last_processed_run_number,
                 last_processed_at = excluded.last_processed_at,
                 updated_at = excluded.updated_at",
            self.pool.placeholders(4)
        );
        let now = Utc::now();
        let values = [
            BindValue::Text(pipeline_id),
            BindValue::Int(run_number),
            BindValue::Timestamp(now),
            BindValue::Timestamp(now),
        ];

        execute_bound(&self.pool, &sql, &values).await?;
        Ok(())
    }

    pub async fn reset_processing_state(&self, pipeline_id: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM metrics_processing_state WHERE pipeline_id = {}",
            self.pool.placeholder(1)
        );

        execute_bound(&self.pool, &sql, &[BindValue::Text(pipeline_id)]).await?;

        tracing::debug!(pipeline_id = %pipeline_id, "Reset processing state for pipeline");
        Ok(())
    }

    pub async fn reset_all_processing_states(&self) -> DomainResult<()> {
        execute_bound(&self.pool, "DELETE FROM metrics_processing_state", &[]).await?;

        tracing::debug!("Reset all processing states");
        Ok(())
    }
}
