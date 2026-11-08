use pipedash_plugin_api::WorkflowParameter;

use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::retry::retry_on_busy;
use crate::infrastructure::database::DatabasePool;

const WORKFLOW_PARAMETERS_TTL_MINUTES: u32 = 10;

impl Repository {
    pub async fn cache_workflow_parameters(
        &self, workflow_id: &str, parameters: &[WorkflowParameter],
    ) -> DomainResult<()> {
        let sql = if self.cache_pool.is_postgres() {
            "INSERT INTO workflow_parameters_cache (workflow_id, parameters_json, cached_at) VALUES ($1, $2, NOW()) ON CONFLICT (workflow_id) DO UPDATE SET parameters_json = EXCLUDED.parameters_json, cached_at = EXCLUDED.cached_at"
        } else {
            "INSERT OR REPLACE INTO workflow_parameters_cache (workflow_id, parameters_json, cached_at) VALUES (?, ?, datetime('now'))"
        };

        retry_on_busy(|| async {
            let parameters_json = serde_json::to_string(parameters).map_err(db_err)?;

            on_pool!(&self.cache_pool, p => sqlx::query(sql)
                .bind(workflow_id)
                .bind(&parameters_json)
                .execute(p)
                .await
                .map(|_| ())
                .map_err(db_err))?;

            Ok(())
        })
        .await
    }

    pub async fn get_cached_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Option<Vec<WorkflowParameter>>> {
        let freshness = match self.cache_pool {
            DatabasePool::Sqlite(_) => format!(
                "cached_at >= datetime('now', '-{WORKFLOW_PARAMETERS_TTL_MINUTES} minutes')"
            ),
            DatabasePool::Postgres(_) => {
                format!("cached_at >= NOW() - INTERVAL '{WORKFLOW_PARAMETERS_TTL_MINUTES} minutes'")
            }
        };
        let sql = format!(
            "SELECT parameters_json FROM workflow_parameters_cache WHERE workflow_id = {} AND {freshness}",
            self.cache_pool.placeholder(1)
        );
        let cached = on_pool!(&self.cache_pool, p => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
            .bind(workflow_id)
            .fetch_optional(p)
            .await
            .map_err(db_err))?;

        let Some(json) = cached else {
            return Ok(None);
        };
        if json.trim().is_empty() {
            return Ok(Some(vec![]));
        }
        serde_json::from_str(&json).map(Some).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to parse workflow parameters: {}", e))
        })
    }

    pub async fn clear_workflow_parameters_cache(&self) -> DomainResult<()> {
        on_pool!(&self.cache_pool, p => sqlx::query("DELETE FROM workflow_parameters_cache")
            .execute(p)
            .await
            .map(|_| ())
            .map_err(db_err))?;

        Ok(())
    }
}
