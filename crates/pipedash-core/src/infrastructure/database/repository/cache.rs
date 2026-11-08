use super::Repository;
use crate::domain::DomainResult;
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

const CLEAR_ORDER: [&str; 3] = [
    "run_history_cache",
    "pipelines_cache",
    "workflow_parameters_cache",
];

impl Repository {
    pub async fn clear_provider_run_history(&self, provider_id: i64) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM run_history_cache WHERE pipeline_id IN (SELECT id FROM pipelines_cache WHERE provider_id = {})",
            self.cache_pool.placeholder(1)
        );
        on_pool!(&self.cache_pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(provider_id)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(db_err))?;

        Ok(())
    }

    async fn count_rows(&self, table: &str) -> DomainResult<i64> {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        on_pool!(&self.cache_pool, p => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .fetch_one(p)
            .await
            .map_err(db_err))
    }

    pub async fn get_pipelines_cache_count(&self) -> DomainResult<i64> {
        self.count_rows("pipelines_cache").await
    }

    pub async fn get_run_history_cache_count(&self) -> DomainResult<i64> {
        self.count_rows("run_history_cache").await
    }

    pub async fn get_workflow_params_cache_count(&self) -> DomainResult<i64> {
        self.count_rows("workflow_parameters_cache").await
    }

    pub async fn clear_pipelines_cache(&self) -> DomainResult<usize> {
        on_pool!(&self.cache_pool, p => sqlx::query("DELETE FROM pipelines_cache")
            .execute(p)
            .await
            .map(|result| result.rows_affected() as usize)
            .map_err(db_err))
    }

    pub async fn clear_all_caches_atomic(&self) -> DomainResult<()> {
        on_pool!(&self.cache_pool, p => {
            let mut tx = p.begin().await.map_err(db_err)?;
            for table in CLEAR_ORDER {
                sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table}")))
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }
            tx.commit().await.map_err(db_err)
        })
    }
}
