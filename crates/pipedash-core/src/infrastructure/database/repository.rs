use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use sqlx::postgres::PgPool;
use sqlx::{
    Row as SqlxRow,
    SqlitePool,
};
use tokio::time::sleep;

use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    ProviderConfig,
};
use crate::infrastructure::deduplication::hash_pipeline_run;
use crate::infrastructure::{
    ConfigBackend,
    TokenStore,
};

#[derive(Clone)]
pub enum DatabasePool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

impl DatabasePool {
    pub fn is_postgres(&self) -> bool {
        matches!(self, DatabasePool::Postgres(_))
    }

    pub fn as_sqlite(&self) -> Option<&SqlitePool> {
        match self {
            DatabasePool::Sqlite(pool) => Some(pool),
            _ => None,
        }
    }

    pub fn as_postgres(&self) -> Option<&PgPool> {
        match self {
            DatabasePool::Postgres(pool) => Some(pool),
            _ => None,
        }
    }
}

const FETCH_STATUS_SUCCESS: &str = "success";
const FETCH_STATUS_ERROR: &str = "error";
const FETCH_STATUS_NEVER: &str = "never";

async fn retry_on_busy<F, Fut, T>(operation: F) -> DomainResult<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = DomainResult<T>>,
{
    const MAX_RETRIES: u32 = 3; // Reduced from 10 to 3 to prevent 5-minute delays
    const INITIAL_DELAY_MS: u64 = 10;

    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::warn!(
                        "SQLite operation succeeded after {} retries ({}ms total)",
                        attempt,
                        INITIAL_DELAY_MS * (2_u64.pow(attempt) - 1)
                    );
                }
                return Ok(result);
            }
            Err(DomainError::DatabaseError(ref msg))
                if (msg.contains("database is locked")
                    || msg.contains("SQLITE_BUSY")
                    || msg.contains("locked"))
                    && attempt < MAX_RETRIES =>
            {
                attempt += 1;
                let delay = INITIAL_DELAY_MS * 2_u64.pow(attempt - 1);

                if attempt > 3 {
                    tracing::warn!(
                        "SQLite busy, retry {}/{} (waiting {}ms)",
                        attempt,
                        MAX_RETRIES,
                        delay
                    );
                }

                sleep(Duration::from_millis(delay)).await;
            }
            Err(e) => {
                if attempt > 0 {
                    tracing::error!("SQLite operation failed after {} retries: {}", attempt, e);
                }
                return Err(e);
            }
        }
    }
}

pub struct Repository {
    config_backend: Arc<dyn ConfigBackend>,
    cache_pool: DatabasePool,
    token_store: Arc<dyn TokenStore>,
}

impl Repository {
    pub fn new(
        config_backend: Arc<dyn ConfigBackend>, cache_pool: DatabasePool,
        token_store: Arc<dyn TokenStore>,
    ) -> Self {
        Self {
            config_backend,
            cache_pool,
            token_store,
        }
    }

    pub fn cache_pool(&self) -> &DatabasePool {
        &self.cache_pool
    }

    pub fn config_backend(&self) -> &Arc<dyn ConfigBackend> {
        &self.config_backend
    }

    pub fn pool(&self) -> &SqlitePool {
        self.cache_pool
            .as_sqlite()
            .expect("Expected SQLite pool but found PostgreSQL")
    }

    #[allow(dead_code)]
    pub(crate) fn datetime_now(&self) -> &'static str {
        match self.cache_pool {
            DatabasePool::Sqlite(_) => "datetime('now')",
            DatabasePool::Postgres(_) => "NOW()",
        }
    }

    fn placeholder(&self, index: usize) -> String {
        match self.cache_pool {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${}", index),
        }
    }

    #[allow(dead_code)]
    fn build_batch_run_history_upsert(&self, batch_size: usize) -> String {
        let columns: [&str; 5] = [
            "pipeline_id",
            "run_number",
            "run_data",
            "fetched_at",
            "run_hash",
        ];
        let params_per_row = 4; // pipeline_id, run_number, run_data, run_hash (fetched_at is datetime)

        match self.cache_pool {
            DatabasePool::Sqlite(_) => {
                let mut param_idx = 0;
                let values_clauses: Vec<String> = (0..batch_size)
                    .map(|_| {
                        let clause = format!(
                            "({}, {}, {}, datetime('now'), {})",
                            self.placeholder(param_idx + 1),
                            self.placeholder(param_idx + 2),
                            self.placeholder(param_idx + 3),
                            self.placeholder(param_idx + 4)
                        );
                        param_idx += params_per_row;
                        clause
                    })
                    .collect();

                format!(
                    "INSERT OR REPLACE INTO run_history_cache ({}) VALUES {}",
                    columns.join(", "),
                    values_clauses.join(", ")
                )
            }
            DatabasePool::Postgres(_) => {
                let mut param_idx = 0;
                let values_clauses: Vec<String> = (0..batch_size)
                    .map(|_| {
                        let clause = format!(
                            "({}, {}, {}, NOW(), {})",
                            self.placeholder(param_idx + 1),
                            self.placeholder(param_idx + 2),
                            self.placeholder(param_idx + 3),
                            self.placeholder(param_idx + 4)
                        );
                        param_idx += params_per_row;
                        clause
                    })
                    .collect();

                format!(
                    "INSERT INTO run_history_cache ({}) VALUES {} ON CONFLICT (pipeline_id, run_number) DO UPDATE SET run_data = EXCLUDED.run_data, fetched_at = EXCLUDED.fetched_at, run_hash = EXCLUDED.run_hash",
                    columns.join(", "),
                    values_clauses.join(", ")
                )
            }
        }
    }

    pub async fn add_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        let provider_id = self.config_backend.create_provider(config).await?;

        self.token_store
            .store_token(provider_id, &config.token)
            .await
            .map_err(|e| {
                tracing::error!(
                    provider_id = provider_id,
                    error = %e,
                    "Failed to store provider token - provider may not work correctly"
                );
                e
            })?;

        tracing::debug!(
            provider_id = provider_id,
            "Provider created and token stored successfully"
        );

        Ok(provider_id)
    }

    pub async fn get_provider(&self, id: i64) -> DomainResult<ProviderConfig> {
        let mut provider = self
            .config_backend
            .get_provider(id)
            .await?
            .ok_or_else(|| DomainError::ProviderNotFound(id.to_string()))?;

        provider.token = self.token_store.get_token(id).await?;

        Ok(provider)
    }

    pub async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        let mut providers = self.config_backend.list_providers().await?;

        for provider in &mut providers {
            if let Some(id) = provider.id {
                provider.token = self.token_store.get_token(id).await.unwrap_or_default();
            }
        }

        Ok(providers)
    }

    pub async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        self.config_backend.update_provider(id, config).await?;

        self.token_store.store_token(id, &config.token).await?;

        Ok(())
    }

    pub async fn update_provider_with_version(
        &self, id: i64, config: &ProviderConfig, expected_version: i64,
    ) -> DomainResult<bool> {
        let cache_pool = self.cache_pool.clone();
        let config_clone = config.clone();
        let token_clone = config.token.clone();
        let token_store = self.token_store.clone();

        retry_on_busy(|| async {
            let config_json = serde_json::to_string(&config_clone.config)
                .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize config: {}", e)))?;

            let rows_affected = match &cache_pool {
                DatabasePool::Sqlite(p) => {
                    sqlx::query(
                        r#"UPDATE providers
                           SET name = ?, provider_type = ?, config_json = ?, refresh_interval = ?,
                               version = version + 1, updated_at = datetime('now')
                           WHERE id = ? AND version = ?"#
                    )
                    .bind(&config_clone.name)
                    .bind(&config_clone.provider_type)
                    .bind(&config_json)
                    .bind(config_clone.refresh_interval)
                    .bind(id)
                    .bind(expected_version)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(format!("Failed to update provider: {}", e)))?
                    .rows_affected()
                }
                DatabasePool::Postgres(p) => {
                    sqlx::query(
                        r#"UPDATE providers
                           SET name = $1, provider_type = $2, config_json = $3, refresh_interval = $4,
                               version = version + 1, updated_at = NOW()
                           WHERE id = $5 AND version = $6"#
                    )
                    .bind(&config_clone.name)
                    .bind(&config_clone.provider_type)
                    .bind(&config_json)
                    .bind(config_clone.refresh_interval)
                    .bind(id)
                    .bind(expected_version)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(format!("Failed to update provider: {}", e)))?
                    .rows_affected()
                }
            };

            let success = rows_affected > 0;

            if success {
                token_store.store_token(id, &token_clone).await?;
            }

            Ok(success)
        }).await
    }

    pub async fn remove_provider(&self, id: i64) -> DomainResult<()> {
        let pipelines = self
            .get_cached_pipelines(Some(id))
            .await
            .unwrap_or_default();
        let pipeline_ids: Vec<String> = pipelines.iter().map(|p| p.id.clone()).collect();

        let run_history_sql = format!(
            "DELETE FROM run_history_cache WHERE pipeline_id IN (SELECT id FROM pipelines_cache WHERE provider_id = {})",
            self.placeholder(1)
        );
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let _ = sqlx::query(&run_history_sql)
                    .bind(id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()));
            }
            DatabasePool::Postgres(p) => {
                let _ = sqlx::query(&run_history_sql)
                    .bind(id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()));
            }
        }

        for pipeline_id in pipeline_ids {
            let workflow_params_sql = format!(
                "DELETE FROM workflow_parameters_cache WHERE workflow_id LIKE {}",
                self.placeholder(1)
            );
            match &self.cache_pool {
                DatabasePool::Sqlite(p) => {
                    let _ = sqlx::query(&workflow_params_sql)
                        .bind(format!("{}%", pipeline_id))
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()));
                }
                DatabasePool::Postgres(p) => {
                    let _ = sqlx::query(&workflow_params_sql)
                        .bind(format!("{}%", pipeline_id))
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()));
                }
            }
        }

        let pipelines_sql = format!(
            "DELETE FROM pipelines_cache WHERE provider_id = {}",
            self.placeholder(1)
        );
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let _ = sqlx::query(&pipelines_sql)
                    .bind(id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()));
            }
            DatabasePool::Postgres(p) => {
                let _ = sqlx::query(&pipelines_sql)
                    .bind(id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()));
            }
        }

        self.config_backend.delete_provider(id).await?;

        let token_store = self.token_store.clone();
        tokio::spawn(async move {
            if let Err(e) = token_store.delete_token(id).await {
                tracing::warn!(
                    provider_id = id,
                    error = %e,
                    "Failed to delete provider token in background - token may persist in keyring"
                );
            } else {
                tracing::debug!(
                    provider_id = id,
                    "Provider token deleted successfully in background"
                );
            }
        });

        Ok(())
    }

    pub async fn update_provider_fetch_status(
        &self, provider_id: i64, success: bool, error: Option<String>,
    ) -> DomainResult<bool> {
        let cache_pool = self.cache_pool.clone();
        let error_clone = error.clone();

        retry_on_busy(|| async {
            let now = chrono::Utc::now();
            let new_status = if success {
                FETCH_STATUS_SUCCESS
            } else {
                FETCH_STATUS_ERROR
            };

            let select_sql = match cache_pool {
                DatabasePool::Sqlite(_) => {
                    "SELECT last_fetch_status, last_fetch_error FROM providers WHERE id = ?".to_string()
                }
                DatabasePool::Postgres(_) => {
                    "SELECT last_fetch_status, last_fetch_error FROM providers WHERE id = $1".to_string()
                }
            };

            let status_changed = match &cache_pool {
                DatabasePool::Sqlite(p) => {
                    let current = sqlx::query(&select_sql)
                        .bind(provider_id)
                        .fetch_optional(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    if let Some(row) = current {
                        let current_status: String =
                            row.try_get("last_fetch_status").unwrap_or_default();
                        let current_error: Option<String> =
                            row.try_get("last_fetch_error").ok().flatten();
                        current_status != new_status || current_error != error_clone
                    } else {
                        true
                    }
                }
                DatabasePool::Postgres(p) => {
                    let current = sqlx::query(&select_sql)
                        .bind(provider_id)
                        .fetch_optional(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    if let Some(row) = current {
                        let current_status: String =
                            row.try_get("last_fetch_status").unwrap_or_default();
                        let current_error: Option<String> =
                            row.try_get("last_fetch_error").ok().flatten();
                        current_status != new_status || current_error != error_clone
                    } else {
                        true
                    }
                }
            };

            if status_changed {
                match &cache_pool {
                    DatabasePool::Sqlite(p) => {
                        let update_sql = "UPDATE providers SET last_fetch_at = ?, last_fetch_status = ?, last_fetch_error = ? WHERE id = ?";
                        sqlx::query(update_sql)
                            .bind(now.to_rfc3339())
                            .bind(new_status)
                            .bind(error_clone.as_ref())
                            .bind(provider_id)
                            .execute(p)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                    DatabasePool::Postgres(p) => {
                        let update_sql = "UPDATE providers SET last_fetch_at = $1, last_fetch_status = $2, last_fetch_error = $3 WHERE id = $4";
                        sqlx::query(update_sql)
                            .bind(now)
                            .bind(new_status)
                            .bind(error_clone.as_ref())
                            .bind(provider_id)
                            .execute(p)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                }
            }

            Ok(status_changed)
        }).await
    }

    pub async fn get_provider_fetch_status(
        &self, provider_id: i64,
    ) -> DomainResult<(String, Option<String>, Option<String>)> {
        let sql = format!(
            "SELECT last_fetch_status, last_fetch_error, last_fetch_at FROM providers WHERE id = {}",
            self.placeholder(1)
        );
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let row = sqlx::query(&sql)
                    .bind(provider_id)
                    .fetch_one(p)
                    .await
                    .map_err(|_e| DomainError::ProviderNotFound(provider_id.to_string()))?;

                let status: String = row
                    .try_get(0)
                    .unwrap_or_else(|_| FETCH_STATUS_NEVER.to_string());
                let error: Option<String> = row.try_get(1).ok().flatten();
                let fetch_at: Option<String> = row.try_get(2).ok().flatten();

                Ok((status, error, fetch_at))
            }
            DatabasePool::Postgres(p) => {
                let row = sqlx::query(&sql)
                    .bind(provider_id)
                    .fetch_one(p)
                    .await
                    .map_err(|_e| DomainError::ProviderNotFound(provider_id.to_string()))?;

                let status: String = row
                    .try_get(0)
                    .unwrap_or_else(|_| FETCH_STATUS_NEVER.to_string());
                let error: Option<String> = row.try_get(1).ok().flatten();
                let fetch_at: Option<String> = row.try_get(2).ok().flatten();

                Ok((status, error, fetch_at))
            }
        }
    }

    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        if let Some(pid) = provider_id {
            let sql = format!(
                "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type
                FROM pipelines_cache
                WHERE provider_id = {}
                ORDER BY last_updated DESC",
                self.placeholder(1)
            );
            match &self.cache_pool {
                DatabasePool::Sqlite(p) => {
                    let rows = sqlx::query(&sql)
                        .bind(pid)
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    rows.iter().map(|row| self.pipeline_from_row(row)).collect()
                }
                DatabasePool::Postgres(p) => {
                    let rows = sqlx::query(&sql)
                        .bind(pid)
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    rows.iter().map(|row| self.pipeline_from_row(row)).collect()
                }
            }
        } else {
            let sql = "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type
                FROM pipelines_cache
                ORDER BY last_updated DESC";
            match &self.cache_pool {
                DatabasePool::Sqlite(p) => {
                    let rows = sqlx::query(sql)
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    rows.iter().map(|row| self.pipeline_from_row(row)).collect()
                }
                DatabasePool::Postgres(p) => {
                    let rows = sqlx::query(sql)
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    rows.iter().map(|row| self.pipeline_from_row(row)).collect()
                }
            }
        }
    }

    pub async fn cache_workflow_parameters(
        &self, workflow_id: &str, parameters: &[pipedash_plugin_api::WorkflowParameter],
    ) -> DomainResult<()> {
        let workflow_id_owned = workflow_id.to_string();
        let parameters_vec = parameters.to_vec();
        let cache_pool = self.cache_pool.clone();
        let is_postgres = cache_pool.is_postgres();

        retry_on_busy(|| async {
            let parameters_json = serde_json::to_string(&parameters_vec)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            let sql = if is_postgres {
                "INSERT INTO workflow_parameters_cache (workflow_id, parameters_json, cached_at) VALUES ($1, $2, NOW()) ON CONFLICT (workflow_id) DO UPDATE SET parameters_json = EXCLUDED.parameters_json, cached_at = EXCLUDED.cached_at"
            } else {
                "INSERT OR REPLACE INTO workflow_parameters_cache (workflow_id, parameters_json, cached_at) VALUES (?, ?, datetime('now'))"
            };

            match &cache_pool {
                DatabasePool::Sqlite(p) => {
                    sqlx::query(sql)
                        .bind(&workflow_id_owned)
                        .bind(&parameters_json)
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }
                DatabasePool::Postgres(p) => {
                    sqlx::query(sql)
                        .bind(&workflow_id_owned)
                        .bind(&parameters_json)
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }
            }

            Ok(())
        }).await
    }

    pub async fn get_cached_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Option<Vec<pipedash_plugin_api::WorkflowParameter>>> {
        let sql = format!(
            "SELECT parameters_json FROM workflow_parameters_cache WHERE workflow_id = {}",
            self.placeholder(1)
        );
        let result = match &self.cache_pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, String>(&sql)
                .bind(workflow_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, String>(&sql)
                .bind(workflow_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
        };

        match result {
            Some(json) => {
                if json.trim().is_empty() {
                    Ok(Some(vec![]))
                } else {
                    let parameters: Vec<pipedash_plugin_api::WorkflowParameter> =
                        serde_json::from_str(&json).map_err(|e| {
                            DomainError::DatabaseError(format!(
                                "Failed to parse workflow parameters: {}",
                                e
                            ))
                        })?;
                    Ok(Some(parameters))
                }
            }
            None => Ok(None),
        }
    }

    pub async fn clear_workflow_parameters_cache(&self) -> DomainResult<()> {
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query("DELETE FROM workflow_parameters_cache")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query("DELETE FROM workflow_parameters_cache")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn cache_run_history(
        &self, pipeline_id: &str, runs: &[PipelineRun],
    ) -> DomainResult<()> {
        if runs.is_empty() {
            return Ok(());
        }

        let start = std::time::Instant::now();
        let pipeline_id_str = pipeline_id.to_string();
        let runs_vec = runs.to_vec();
        let pool = self.cache_pool.clone();

        retry_on_busy(|| {
            let pipeline_id_clone = pipeline_id_str.clone();
            let runs_clone = runs_vec.clone();
            let pool_clone = pool.clone();
            async move {

                const BATCH_SIZE: usize = 100;

                match &pool_clone {
                    DatabasePool::Sqlite(p) => {
                        let mut tx = p.begin().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        for chunk in runs_clone.chunks(BATCH_SIZE) {
                            let prepared_data: Vec<(i64, String, String)> = chunk
                                .iter()
                                .map(|run| {
                                    let run_data = serde_json::to_string(run)
                                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                                    let status_str = run.status.as_str();
                                    let run_hash = hash_pipeline_run(
                                        run.run_number,
                                        status_str,
                                        run.branch.as_deref(),
                                        &run.started_at.to_rfc3339(),
                                        run.duration_seconds,
                                        run.commit_sha.as_deref(),
                                    );
                                    Ok((run.run_number, run_data, run_hash))
                                })
                                .collect::<Result<Vec<_>, DomainError>>()?;

                            let values_clause = prepared_data
                                .iter()
                                .map(|_| "(?, ?, ?, datetime('now'), ?)")
                                .collect::<Vec<_>>()
                                .join(", ");

                            let sql = format!(
                                "INSERT OR REPLACE INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash) VALUES {}",
                                values_clause
                            );

                            let mut query = sqlx::query(&sql);
                            for (run_number, run_data, run_hash) in &prepared_data {
                                query = query
                                    .bind(&pipeline_id_clone)
                                    .bind(run_number)
                                    .bind(run_data)
                                    .bind(run_hash);
                            }

                            query.execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        }

                        tx.commit().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                    DatabasePool::Postgres(p) => {
                        let mut tx = p.begin().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        for chunk in runs_clone.chunks(BATCH_SIZE) {
                            let prepared_data: Vec<(i64, String, String)> = chunk
                                .iter()
                                .map(|run| {
                                    let run_data = serde_json::to_string(run)
                                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                                    let status_str = run.status.as_str();
                                    let run_hash = hash_pipeline_run(
                                        run.run_number,
                                        status_str,
                                        run.branch.as_deref(),
                                        &run.started_at.to_rfc3339(),
                                        run.duration_seconds,
                                        run.commit_sha.as_deref(),
                                    );
                                    Ok((run.run_number, run_data, run_hash))
                                })
                                .collect::<Result<Vec<_>, DomainError>>()?;

                            let mut param_idx = 0;
                            let values_clauses: Vec<String> = prepared_data
                                .iter()
                                .map(|_| {
                                    let clause = format!(
                                        "(${}, ${}, ${}, NOW(), ${})",
                                        param_idx + 1, param_idx + 2, param_idx + 3, param_idx + 4
                                    );
                                    param_idx += 4;
                                    clause
                                })
                                .collect();

                            let sql = format!(
                                "INSERT INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash) VALUES {} ON CONFLICT (pipeline_id, run_number) DO UPDATE SET run_data = EXCLUDED.run_data, fetched_at = EXCLUDED.fetched_at, run_hash = EXCLUDED.run_hash",
                                values_clauses.join(", ")
                            );

                            let mut query = sqlx::query(&sql);
                            for (run_number, run_data, run_hash) in &prepared_data {
                                query = query
                                    .bind(&pipeline_id_clone)
                                    .bind(run_number)
                                    .bind(run_data)
                                    .bind(run_hash);
                            }

                            query.execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        }

                        tx.commit().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                }

                Ok(())
            }
        })
        .await?;

        let elapsed = start.elapsed();
        tracing::debug!(
            pipeline_id = pipeline_id,
            runs_count = runs.len(),
            elapsed_ms = elapsed.as_millis(),
            "Cached run history (batch insert)"
        );

        Ok(())
    }

    pub async fn get_cached_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let sql = format!(
            "SELECT run_data FROM run_history_cache
             WHERE pipeline_id = {}
             ORDER BY run_number DESC
             LIMIT {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let rows = match &self.cache_pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, String>(&sql)
                .bind(pipeline_id)
                .bind(limit as i64)
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, String>(&sql)
                .bind(pipeline_id)
                .bind(limit as i64)
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
        };

        let runs: Vec<PipelineRun> = rows
            .iter()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect();

        Ok(runs)
    }

    pub async fn clear_cached_run_history(&self, pipeline_id: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM run_history_cache WHERE pipeline_id = {}",
            self.placeholder(1)
        );
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query(&sql)
                    .bind(pipeline_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query(&sql)
                    .bind(pipeline_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn clear_all_run_history_cache(&self) -> DomainResult<()> {
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query("DELETE FROM run_history_cache")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query("DELETE FROM run_history_cache")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn get_cached_runs_with_hashes(
        &self, pipeline_id: &str,
    ) -> DomainResult<HashMap<i64, (PipelineRun, String)>> {
        let mut result = HashMap::new();
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let sql = format!(
                    "SELECT run_number, run_data, run_hash FROM run_history_cache WHERE pipeline_id = {}",
                    self.placeholder(1)
                );
                let rows = sqlx::query(&sql)
                    .bind(pipeline_id)
                    .fetch_all(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for row in rows {
                    let run_number: i64 = row
                        .try_get(0)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let run_data: String = row
                        .try_get(1)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let run_hash: String = row
                        .try_get(2)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    if let Ok(run) = serde_json::from_str::<PipelineRun>(&run_data) {
                        result.insert(run_number, (run, run_hash));
                    }
                }
            }
            DatabasePool::Postgres(p) => {
                let sql = format!(
                    "SELECT run_number::BIGINT, run_data, run_hash FROM run_history_cache WHERE pipeline_id = {}",
                    self.placeholder(1)
                );
                let rows = sqlx::query(&sql)
                    .bind(pipeline_id)
                    .fetch_all(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for row in rows {
                    let run_number: i64 = row
                        .try_get(0)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let run_data: String = row
                        .try_get(1)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let run_hash: String = row
                        .try_get(2)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    if let Ok(run) = serde_json::from_str::<PipelineRun>(&run_data) {
                        result.insert(run_number, (run, run_hash));
                    }
                }
            }
        }

        Ok(result)
    }

    pub async fn merge_run_cache(
        &self, pipeline_id: &str, new_runs: Vec<PipelineRun>, changed_runs: Vec<PipelineRun>,
        deleted_run_numbers: Vec<i64>,
    ) -> DomainResult<()> {
        let start = std::time::Instant::now();
        let pipeline_id_str = pipeline_id.to_string();
        let pool = self.cache_pool.clone();

        let new_runs_count = new_runs.len();
        let changed_runs_count = changed_runs.len();
        let deleted_runs_count = deleted_run_numbers.len();

        retry_on_busy(move || {
            let pipeline_id_clone = pipeline_id_str.clone();
            let new_runs_clone = new_runs.clone();
            let changed_runs_clone = changed_runs.clone();
            let deleted_clone = deleted_run_numbers.clone();
            let pool_clone = pool.clone();

            async move {
                const BATCH_SIZE: usize = 100;
                const DELETE_BATCH_SIZE: usize = 100;

                let all_runs: Vec<_> = new_runs_clone.iter().chain(changed_runs_clone.iter()).collect();

                match &pool_clone {
                    DatabasePool::Sqlite(p) => {
                        let mut tx = p.begin().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        if !all_runs.is_empty() {
                            for chunk in all_runs.chunks(BATCH_SIZE) {
                                let prepared_data: Vec<(i64, String, String)> = chunk
                                    .iter()
                                    .map(|run| {
                                        let run_data = serde_json::to_string(run)
                                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                                        let status_str = run.status.as_str();
                                        let run_hash = hash_pipeline_run(
                                            run.run_number,
                                            status_str,
                                            run.branch.as_deref(),
                                            &run.started_at.to_rfc3339(),
                                            run.duration_seconds,
                                            run.commit_sha.as_deref(),
                                        );
                                        Ok((run.run_number, run_data, run_hash))
                                    })
                                    .collect::<Result<Vec<_>, DomainError>>()?;

                                let values_clause = prepared_data
                                    .iter()
                                    .map(|_| "(?, ?, ?, datetime('now'), ?)")
                                    .collect::<Vec<_>>()
                                    .join(", ");

                                let sql = format!(
                                    "INSERT OR REPLACE INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash) VALUES {}",
                                    values_clause
                                );

                                let mut query = sqlx::query(&sql);
                                for (run_number, run_data, run_hash) in &prepared_data {
                                    query = query
                                        .bind(&pipeline_id_clone)
                                        .bind(run_number)
                                        .bind(run_data)
                                        .bind(run_hash);
                                }

                                query.execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        if !deleted_clone.is_empty() {
                            for chunk in deleted_clone.chunks(DELETE_BATCH_SIZE) {
                                let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                                let sql = format!(
                                    "DELETE FROM run_history_cache WHERE pipeline_id = ? AND run_number IN ({})",
                                    placeholders
                                );

                                let mut query = sqlx::query(&sql).bind(&pipeline_id_clone);
                                for run_number in chunk {
                                    query = query.bind(run_number);
                                }

                                query.execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        tx.commit().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                    DatabasePool::Postgres(p) => {
                        let mut tx = p.begin().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        if !all_runs.is_empty() {
                            for chunk in all_runs.chunks(BATCH_SIZE) {
                                let prepared_data: Vec<(i64, String, String)> = chunk
                                    .iter()
                                    .map(|run| {
                                        let run_data = serde_json::to_string(run)
                                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                                        let status_str = run.status.as_str();
                                        let run_hash = hash_pipeline_run(
                                            run.run_number,
                                            status_str,
                                            run.branch.as_deref(),
                                            &run.started_at.to_rfc3339(),
                                            run.duration_seconds,
                                            run.commit_sha.as_deref(),
                                        );
                                        Ok((run.run_number, run_data, run_hash))
                                    })
                                    .collect::<Result<Vec<_>, DomainError>>()?;

                                let mut param_idx = 0;
                                let values_clauses: Vec<String> = prepared_data
                                    .iter()
                                    .map(|_| {
                                        let clause = format!(
                                            "(${}, ${}, ${}, NOW(), ${})",
                                            param_idx + 1, param_idx + 2, param_idx + 3, param_idx + 4
                                        );
                                        param_idx += 4;
                                        clause
                                    })
                                    .collect();

                                let sql = format!(
                                    "INSERT INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash) VALUES {} ON CONFLICT (pipeline_id, run_number) DO UPDATE SET run_data = EXCLUDED.run_data, fetched_at = EXCLUDED.fetched_at, run_hash = EXCLUDED.run_hash",
                                    values_clauses.join(", ")
                                );

                                let mut query = sqlx::query(&sql);
                                for (run_number, run_data, run_hash) in &prepared_data {
                                    query = query
                                        .bind(&pipeline_id_clone)
                                        .bind(run_number)
                                        .bind(run_data)
                                        .bind(run_hash);
                                }

                                query.execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        if !deleted_clone.is_empty() {
                            for chunk in deleted_clone.chunks(DELETE_BATCH_SIZE) {
                                let mut param_idx = 1;
                                let placeholders = chunk.iter().map(|_| {
                                    let p = format!("${}", param_idx);
                                    param_idx += 1;
                                    p
                                }).collect::<Vec<_>>().join(", ");

                                let sql = format!(
                                    "DELETE FROM run_history_cache WHERE pipeline_id = ${} AND run_number IN ({})",
                                    param_idx,
                                    placeholders
                                );

                                let mut query = sqlx::query(&sql);
                                for run_number in chunk {
                                    query = query.bind(run_number);
                                }
                                query = query.bind(&pipeline_id_clone);

                                query.execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        tx.commit().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                }

                Ok(())
            }
        })
        .await?;

        let elapsed = start.elapsed();
        tracing::debug!(
            pipeline_id = pipeline_id,
            new_runs = new_runs_count,
            changed_runs = changed_runs_count,
            deleted_runs = deleted_runs_count,
            elapsed_ms = elapsed.as_millis(),
            "Merged run cache (batch operations)"
        );

        Ok(())
    }

    pub async fn update_pipelines_cache(
        &self, provider_id: i64, new_pipelines: &[Pipeline],
    ) -> DomainResult<()> {
        let new_pipelines_vec = new_pipelines.to_vec();
        let pool = self.cache_pool.clone();
        let is_postgres = pool.is_postgres();

        retry_on_busy(move || {
            let new_pipelines_clone = new_pipelines_vec.clone();
            let pool_clone = pool.clone();
            let is_pg = is_postgres;
            async move {
                let select_sql = if is_pg {
                    "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type
                    FROM pipelines_cache
                    WHERE provider_id = $1"
                } else {
                    "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type
                    FROM pipelines_cache
                    WHERE provider_id = ?"
                };

                let update_sql = if is_pg {
                    "UPDATE pipelines_cache
                     SET provider_id = $1, name = $2, status = $3, repository = $4, branch = $5,
                         workflow_file = $6, last_run = $7, last_updated = $8, metadata_json = $9, provider_type = $10
                     WHERE id = $11"
                } else {
                    "UPDATE pipelines_cache
                     SET provider_id = ?, name = ?, status = ?, repository = ?, branch = ?,
                         workflow_file = ?, last_run = ?, last_updated = ?, metadata_json = ?, provider_type = ?
                     WHERE id = ?"
                };

                let insert_sql = if is_pg {
                    "INSERT INTO pipelines_cache
                     (id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, metadata_json, provider_type)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
                } else {
                    "INSERT INTO pipelines_cache
                     (id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, metadata_json, provider_type)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                };

                let delete_sql = if is_pg {
                    "DELETE FROM pipelines_cache WHERE id = $1"
                } else {
                    "DELETE FROM pipelines_cache WHERE id = ?"
                };

                match &pool_clone {
                    DatabasePool::Sqlite(p) => {
                        let mut tx = p.begin().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        let existing_rows = sqlx::query(select_sql)
                            .bind(provider_id)
                            .fetch_all(&mut *tx)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                let mut existing: HashMap<String, Pipeline> = HashMap::new();
                for row in existing_rows.iter() {
                    let id: String = row
                        .try_get(0)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let provider_id_val: i64 = row
                        .try_get(1)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let name: String = row
                        .try_get(2)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let status_str: String = row
                        .try_get(3)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let repository: String = row
                        .try_get(4)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let branch: Option<String> = row
                        .try_get(5)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let workflow_file: Option<String> = row
                        .try_get(6)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let last_run_str: Option<String> = row
                        .try_get(7)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let last_updated_str: String = row
                        .try_get(8)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let provider_type: String = row
                        .try_get(9)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    let status: PipelineStatus = if status_str.trim().is_empty() {
                        PipelineStatus::Pending
                    } else if status_str.starts_with('"') {
                        serde_json::from_str(&status_str).map_err(|e| {
                            DomainError::DatabaseError(format!(
                                "Failed to parse status '{}': {}",
                                status_str, e
                            ))
                        })?
                    } else {
                        let quoted = format!("\"{}\"", status_str);
                        serde_json::from_str(&quoted).map_err(|e| {
                            DomainError::DatabaseError(format!(
                                "Failed to parse status '{}': {}",
                                status_str, e
                            ))
                        })?
                    };
                    let last_run = last_run_str
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc));
                    let last_updated = DateTime::parse_from_rfc3339(&last_updated_str)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                        .with_timezone(&Utc);

                    existing.insert(
                        id.clone(),
                        Pipeline {
                            id,
                            provider_id: provider_id_val,
                            provider_type,
                            name,
                            status,
                            last_run,
                            last_updated,
                            repository,
                            branch,
                            workflow_file,
                            metadata: std::collections::HashMap::new(),
                        },
                    );
                }

                        let new_ids: HashMap<String, &Pipeline> =
                            new_pipelines_clone.iter().map(|p| (p.id.clone(), p)).collect();

                        for pipeline in &new_pipelines_clone {
                            if let Some(old) = existing.get(&pipeline.id) {
                                if old.status != pipeline.status
                                    || old.last_run != pipeline.last_run
                                    || old.name != pipeline.name
                                    || old.provider_id != provider_id
                                    || old.provider_type != pipeline.provider_type
                                {
                                    sqlx::query(update_sql)
                                        .bind(provider_id)
                                        .bind(&pipeline.name)
                                        .bind(pipeline.status.as_str())
                                        .bind(&pipeline.repository)
                                        .bind(&pipeline.branch)
                                        .bind(&pipeline.workflow_file)
                                        .bind(pipeline.last_run.as_ref().map(|dt| dt.to_rfc3339()))
                                        .bind(Utc::now().to_rfc3339())
                                        .bind("{}")
                                        .bind(&pipeline.provider_type)
                                        .bind(&pipeline.id)
                                        .execute(&mut *tx)
                                        .await
                                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                                }
                            } else {
                                sqlx::query(insert_sql)
                                    .bind(&pipeline.id)
                                    .bind(provider_id)
                                    .bind(&pipeline.name)
                                    .bind(pipeline.status.as_str())
                                    .bind(&pipeline.repository)
                                    .bind(&pipeline.branch)
                                    .bind(&pipeline.workflow_file)
                                    .bind(pipeline.last_run.as_ref().map(|dt| dt.to_rfc3339()))
                                    .bind(Utc::now().to_rfc3339())
                                    .bind("{}")
                                    .bind(&pipeline.provider_type)
                                    .execute(&mut *tx)
                                    .await
                                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        for (old_id, _) in existing.iter() {
                            if !new_ids.contains_key(old_id) {
                                sqlx::query(delete_sql)
                                    .bind(old_id)
                                    .execute(&mut *tx)
                                    .await
                                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        tx.commit().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                    DatabasePool::Postgres(p) => {
                        let mut tx = p.begin().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        let existing_rows = sqlx::query(select_sql)
                            .bind(provider_id)
                            .fetch_all(&mut *tx)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        let mut existing: HashMap<String, Pipeline> = HashMap::new();
                        for row in existing_rows.iter() {
                            let id: String = row.try_get(0).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let provider_id_val: i64 = row.try_get(1).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let name: String = row.try_get(2).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let status_str: String = row.try_get(3).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let repository: String = row.try_get(4).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let branch: Option<String> = row.try_get(5).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let workflow_file: Option<String> = row.try_get(6).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let last_run: Option<DateTime<Utc>> = row.try_get(7).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let last_updated: DateTime<Utc> = row.try_get(8).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            let provider_type: String = row.try_get(9).map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                            let status: PipelineStatus = if status_str.trim().is_empty() {
                                PipelineStatus::Pending
                            } else if status_str.starts_with('"') {
                                serde_json::from_str(&status_str).map_err(|e| DomainError::DatabaseError(format!("Failed to parse status '{}': {}", status_str, e)))?
                            } else {
                                let quoted = format!("\"{}\"", status_str);
                                serde_json::from_str(&quoted).map_err(|e| DomainError::DatabaseError(format!("Failed to parse status '{}': {}", status_str, e)))?
                            };

                            existing.insert(id.clone(), Pipeline {
                                id, provider_id: provider_id_val, provider_type, name, status,
                                last_run, last_updated, repository, branch, workflow_file,
                                metadata: std::collections::HashMap::new(),
                            });
                        }

                        let new_ids: HashMap<String, &Pipeline> = new_pipelines_clone.iter().map(|p| (p.id.clone(), p)).collect();

                        for pipeline in &new_pipelines_clone {
                            let now = Utc::now();
                            if let Some(old) = existing.get(&pipeline.id) {
                                if old.status != pipeline.status || old.last_run != pipeline.last_run || old.name != pipeline.name || old.provider_id != provider_id || old.provider_type != pipeline.provider_type {
                                    sqlx::query(update_sql)
                                        .bind(provider_id).bind(&pipeline.name).bind(pipeline.status.as_str()).bind(&pipeline.repository).bind(&pipeline.branch)
                                        .bind(&pipeline.workflow_file).bind(pipeline.last_run).bind(now).bind("{}").bind(&pipeline.provider_type).bind(&pipeline.id)
                                        .execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                                }
                            } else {
                                sqlx::query(insert_sql)
                                    .bind(&pipeline.id).bind(provider_id).bind(&pipeline.name).bind(pipeline.status.as_str()).bind(&pipeline.repository).bind(&pipeline.branch)
                                    .bind(&pipeline.workflow_file).bind(pipeline.last_run).bind(now).bind("{}").bind(&pipeline.provider_type)
                                    .execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        for (old_id, _) in existing.iter() {
                            if !new_ids.contains_key(old_id) {
                                sqlx::query(delete_sql).bind(old_id).execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        tx.commit().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                }
                Ok(())
            }
        })
        .await
    }

    pub async fn get_pipelines_cache_count(&self) -> DomainResult<i64> {
        let count = match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pipelines_cache")
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pipelines_cache")
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
        };
        Ok(count)
    }

    pub async fn get_run_history_cache_count(&self) -> DomainResult<i64> {
        let count = match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM run_history_cache")
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM run_history_cache")
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
        };
        Ok(count)
    }

    pub async fn get_workflow_params_cache_count(&self) -> DomainResult<i64> {
        let count = match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workflow_parameters_cache")
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workflow_parameters_cache")
                    .fetch_one(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            }
        };
        Ok(count)
    }

    pub async fn clear_pipelines_cache(&self) -> DomainResult<usize> {
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let result = sqlx::query("DELETE FROM pipelines_cache")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                Ok(result.rows_affected() as usize)
            }
            DatabasePool::Postgres(p) => {
                let result = sqlx::query("DELETE FROM pipelines_cache")
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                Ok(result.rows_affected() as usize)
            }
        }
    }

    pub async fn clear_all_caches_atomic(&self) -> DomainResult<()> {
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                sqlx::query("DELETE FROM run_history_cache")
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                sqlx::query("DELETE FROM pipelines_cache")
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                sqlx::query("DELETE FROM workflow_parameters_cache")
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                sqlx::query("DELETE FROM run_history_cache")
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                sqlx::query("DELETE FROM pipelines_cache")
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                sqlx::query("DELETE FROM workflow_parameters_cache")
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        self.config_backend
            .get_table_preferences(provider_id, table_id)
            .await
    }

    pub async fn upsert_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()> {
        self.config_backend
            .set_table_preferences(provider_id, table_id, preferences_json)
            .await
    }

    pub async fn store_provider_permissions(
        &self, provider_id: i64, status: &pipedash_plugin_api::PermissionStatus,
    ) -> DomainResult<()> {
        let mut permissions_map = HashMap::new();
        for perm_check in &status.permissions {
            permissions_map.insert(perm_check.permission.name.clone(), perm_check.granted);
        }

        let stored_permissions = crate::infrastructure::config_backend::StoredPermissions {
            permissions: permissions_map,
            last_checked: status.checked_at,
        };

        self.config_backend
            .store_permissions(provider_id, &stored_permissions)
            .await
    }

    pub async fn get_provider_permissions(
        &self, provider_id: i64,
    ) -> DomainResult<Option<pipedash_plugin_api::PermissionStatus>> {
        let stored_perms = self.config_backend.get_permissions(provider_id).await?;

        match stored_perms {
            Some(stored) => {
                let mut permissions = Vec::new();
                let mut all_granted = true;

                for (name, granted) in stored.permissions {
                    let permission = pipedash_plugin_api::Permission {
                        name,
                        description: String::new(),
                        required: false,
                    };

                    permissions.push(pipedash_plugin_api::PermissionCheck {
                        permission,
                        granted,
                    });

                    if !granted {
                        all_granted = false;
                    }
                }

                Ok(Some(pipedash_plugin_api::PermissionStatus {
                    permissions,
                    all_granted,
                    checked_at: stored.last_checked,
                    metadata: HashMap::new(),
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn get_cached_run_count(&self, pipeline_id: &str) -> DomainResult<usize> {
        let sql = format!(
            "SELECT COUNT(*) FROM run_history_cache WHERE pipeline_id = {}",
            self.placeholder(1)
        );
        let count = match &self.cache_pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, i64>(&sql)
                .bind(pipeline_id)
                .fetch_one(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, i64>(&sql)
                .bind(pipeline_id)
                .fetch_one(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
        };

        Ok(count as usize)
    }

    pub async fn get_paginated_runs(
        &self, pipeline_id: &str, page: usize, page_size: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let offset = (page - 1) * page_size;

        let sql = format!(
            "SELECT run_data FROM run_history_cache
             WHERE pipeline_id = {}
             ORDER BY run_number DESC
             LIMIT {} OFFSET {}",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3)
        );
        let runs_json = match &self.cache_pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, String>(&sql)
                .bind(pipeline_id)
                .bind(page_size as i64)
                .bind(offset as i64)
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, String>(&sql)
                .bind(pipeline_id)
                .bind(page_size as i64)
                .bind(offset as i64)
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
        };

        let runs: Vec<PipelineRun> = runs_json
            .into_iter()
            .filter_map(|json| serde_json::from_str::<PipelineRun>(&json).ok())
            .collect();

        Ok(runs)
    }

    fn pipeline_from_row<'r, R>(&self, row: &'r R) -> DomainResult<Pipeline>
    where
        R: SqlxRow,
        usize: sqlx::ColumnIndex<R>,
        String: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        i64: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        Option<String>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        DateTime<Utc>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
        Option<DateTime<Utc>>: sqlx::Type<R::Database> + sqlx::Decode<'r, R::Database>,
    {
        let id: String = row
            .try_get(0)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let provider_id: i64 = row
            .try_get(1)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let name: String = row
            .try_get(2)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let status_str: String = row
            .try_get(3)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let repository: String = row
            .try_get(4)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let branch: Option<String> = row
            .try_get(5)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let workflow_file: Option<String> = row
            .try_get(6)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let status: PipelineStatus = if status_str.trim().is_empty() {
            PipelineStatus::Pending
        } else if status_str.starts_with('"') {
            serde_json::from_str(&status_str).map_err(|e| {
                DomainError::DatabaseError(format!(
                    "Failed to parse status '{}': {}",
                    status_str, e
                ))
            })?
        } else {
            let quoted = format!("\"{}\"", status_str);
            serde_json::from_str(&quoted).map_err(|e| {
                DomainError::DatabaseError(format!(
                    "Failed to parse status '{}': {}",
                    status_str, e
                ))
            })?
        };

        let (last_run, last_updated) = match &self.cache_pool {
            DatabasePool::Sqlite(_) => {
                let last_run_str: Option<String> = row
                    .try_get(7)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                let last_updated_str: String = row
                    .try_get(8)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                let last_run = last_run_str
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc));
                let last_updated = DateTime::parse_from_rfc3339(&last_updated_str)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                    .with_timezone(&Utc);

                (last_run, last_updated)
            }
            DatabasePool::Postgres(_) => {
                let last_run: Option<DateTime<Utc>> = row
                    .try_get(7)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                let last_updated: DateTime<Utc> = row
                    .try_get(8)
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                (last_run, last_updated)
            }
        };

        let provider_type: String = row
            .try_get(9)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(Pipeline {
            id,
            provider_id,
            provider_type,
            name,
            status,
            last_run,
            last_updated,
            repository,
            branch,
            workflow_file,
            metadata: std::collections::HashMap::new(),
        })
    }
}
