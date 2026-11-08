use std::collections::HashMap;

use pipedash_plugin_api::RunHistoryOrder;
use sqlx::Row as SqlxRow;

use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
    PipelineRun,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

fn run_order(order: RunHistoryOrder) -> &'static str {
    match order {
        RunHistoryOrder::RunNumber => "run_number DESC",
        RunHistoryOrder::StartedAt => "started_at_ms DESC, run_number DESC",
    }
}

impl Repository {
    async fn run_window(
        &self, pipeline_id: &str, order: RunHistoryOrder, limit: usize, offset: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let sql = format!(
            "SELECT run_data FROM run_history_cache
             WHERE pipeline_id = {}
             ORDER BY {}
             LIMIT {} OFFSET {}",
            self.cache_pool.placeholder(1),
            run_order(order),
            self.cache_pool.placeholder(2),
            self.cache_pool.placeholder(3)
        );
        let rows = on_pool!(&self.cache_pool, p => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
            .bind(pipeline_id)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(p)
            .await
            .map_err(db_err))?;

        Ok(rows
            .iter()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect())
    }

    pub(crate) async fn list_run_history_pipeline_ids(&self) -> DomainResult<Vec<String>> {
        on_pool!(&self.cache_pool, p => sqlx::query_scalar("SELECT DISTINCT pipeline_id FROM run_history_cache")
            .fetch_all(p)
            .await
            .map_err(db_err))
    }

    pub async fn get_cached_run_history(
        &self, pipeline_id: &str, limit: usize, order: RunHistoryOrder,
    ) -> DomainResult<Vec<PipelineRun>> {
        self.run_window(pipeline_id, order, limit, 0).await
    }

    pub async fn get_paginated_runs(
        &self, pipeline_id: &str, page: usize, page_size: usize, order: RunHistoryOrder,
    ) -> DomainResult<Vec<PipelineRun>> {
        self.run_window(pipeline_id, order, page_size, (page - 1) * page_size)
            .await
    }

    pub async fn get_cached_run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Option<PipelineRun>> {
        let sql = format!(
            "SELECT run_data FROM run_history_cache WHERE pipeline_id = {} AND run_number = {}",
            self.cache_pool.placeholder(1),
            self.cache_pool.placeholder(2)
        );
        let row = on_pool!(&self.cache_pool, p => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
            .bind(pipeline_id)
            .bind(run_number)
            .fetch_optional(p)
            .await
            .map_err(db_err))?;

        row.map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|error| DomainError::DatabaseError(format!("Invalid cached run: {error}")))
    }

    pub async fn clear_cached_run_history(&self, pipeline_id: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM run_history_cache WHERE pipeline_id = {}",
            self.cache_pool.placeholder(1)
        );
        on_pool!(&self.cache_pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(pipeline_id)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(db_err))?;

        Ok(())
    }

    pub async fn clear_all_run_history_cache(&self) -> DomainResult<()> {
        on_pool!(&self.cache_pool, p => sqlx::query("DELETE FROM run_history_cache")
            .execute(p)
            .await
            .map(|_| ())
            .map_err(db_err))?;

        Ok(())
    }

    pub async fn get_cached_runs_with_hashes(
        &self, pipeline_id: &str,
    ) -> DomainResult<HashMap<i64, (PipelineRun, String)>> {
        let run_number = if self.cache_pool.is_postgres() {
            "run_number::BIGINT"
        } else {
            "run_number"
        };
        let sql = format!(
            "SELECT {run_number}, run_data, run_hash FROM run_history_cache WHERE pipeline_id = {}",
            self.cache_pool.placeholder(1)
        );
        let rows: Vec<(i64, String, String)> = on_pool!(&self.cache_pool, p => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(pipeline_id)
                .fetch_all(p)
                .await
                .map_err(db_err)?;
            rows.iter()
                .map(|row| Ok::<_, sqlx::Error>((row.try_get(0)?, row.try_get(1)?, row.try_get(2)?)))
                .collect::<Result<_, sqlx::Error>>()
                .map_err(db_err)?
        });

        Ok(rows
            .into_iter()
            .filter_map(|(run_number, run_data, run_hash)| {
                let run = serde_json::from_str::<PipelineRun>(&run_data).ok()?;
                Some((run_number, (run, run_hash)))
            })
            .collect())
    }

    pub async fn get_cached_run_count(&self, pipeline_id: &str) -> DomainResult<usize> {
        let sql = format!(
            "SELECT COUNT(*) FROM run_history_cache WHERE pipeline_id = {}",
            self.cache_pool.placeholder(1)
        );
        let count = on_pool!(&self.cache_pool, p => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .bind(pipeline_id)
            .fetch_one(p)
            .await
            .map_err(db_err))?;

        Ok(count as usize)
    }
}
