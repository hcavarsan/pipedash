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
    ConfigState,
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

pub(crate) const FETCH_STATUS_SUCCESS: &str = "success";
pub(crate) const FETCH_STATUS_ERROR: &str = "error";
pub(crate) const FETCH_STATUS_NEVER: &str = "never";
const WORKFLOW_PARAMETERS_TTL_MINUTES: u32 = 10;

async fn retry_on_busy<F, Fut, T>(operation: F) -> DomainResult<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = DomainResult<T>>,
{
    const MAX_RETRIES: u32 = 3; // Reduced from 10 to 3 to prevent 5-minute
                                // delays
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

fn run_order(pipeline_id: &str) -> &'static str {
    if pipeline_id.starts_with("fluxcd__") {
        "started_at_ms DESC, run_number DESC"
    } else {
        "run_number DESC"
    }
}

fn run_cache_row(run: &PipelineRun) -> DomainResult<(i64, String, String, i64)> {
    let run_data =
        serde_json::to_string(run).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
    let run_hash = hash_pipeline_run(
        run.run_number,
        run.status.as_str(),
        run.branch.as_deref(),
        &run.started_at.to_rfc3339(),
        run.duration_seconds,
        run.commit_sha.as_deref(),
    );

    Ok((
        run.run_number,
        run_data,
        run_hash,
        run.started_at.timestamp_millis(),
    ))
}

pub struct ProviderFetchStatus {
    pub status: String,
    pub error: Option<String>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub succeeded_at: Option<DateTime<Utc>>,
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

        provider.token = match self.token_store.get_token(id).await {
            Ok(token) => token,
            Err(DomainError::NotFound(_)) => String::new(),
            Err(e) => return Err(e),
        };

        Ok(provider)
    }

    pub async fn store_token(&self, id: i64, token: &str) -> DomainResult<()> {
        self.token_store.store_token(id, token).await
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

    pub async fn list_provider_ids(&self) -> DomainResult<Vec<i64>> {
        Ok(self
            .config_backend
            .list_providers()
            .await?
            .into_iter()
            .filter_map(|provider| provider.id)
            .collect())
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

    pub async fn remove_provider(&self, id: i64, config_state: &ConfigState) -> DomainResult<()> {
        let config = self.config_backend.get_provider(id).await?;
        self.token_store.delete_token(id).await?;
        if let Some(config) = config {
            config_state.remove_provider(&config.name).await?;
        }

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
                let _ = sqlx::query(sqlx::AssertSqlSafe(run_history_sql))
                    .bind(id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()));
            }
            DatabasePool::Postgres(p) => {
                let _ = sqlx::query(sqlx::AssertSqlSafe(run_history_sql))
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
                    let _ = sqlx::query(sqlx::AssertSqlSafe(workflow_params_sql))
                        .bind(format!("{}%", pipeline_id))
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()));
                }
                DatabasePool::Postgres(p) => {
                    let _ = sqlx::query(sqlx::AssertSqlSafe(workflow_params_sql))
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
                let _ = sqlx::query(sqlx::AssertSqlSafe(pipelines_sql))
                    .bind(id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()));
            }
            DatabasePool::Postgres(p) => {
                let _ = sqlx::query(sqlx::AssertSqlSafe(pipelines_sql))
                    .bind(id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()));
            }
        }

        if let Err(e) = super::PipelineViewRepository::new(self.cache_pool.clone())
            .remove_provider(id)
            .await
        {
            tracing::warn!("Failed to remove provider {id} from pipeline views: {e}");
        }

        self.config_backend.delete_provider(id).await?;

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
                    "SELECT last_fetch_status, last_fetch_error FROM providers WHERE id = ?"
                }
                DatabasePool::Postgres(_) => {
                    "SELECT last_fetch_status, last_fetch_error FROM providers WHERE id = $1"
                }
            };

            let status_changed = match &cache_pool {
                DatabasePool::Sqlite(p) => {
                    let current = sqlx::query(select_sql)
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
                    let current = sqlx::query(select_sql)
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

            match &cache_pool {
                DatabasePool::Sqlite(p) => {
                    let update_sql = "UPDATE providers SET last_fetch_at = ?, last_fetch_status = ?, last_fetch_error = ?, last_success_at = COALESCE(?, last_success_at) WHERE id = ?";
                    sqlx::query(update_sql)
                        .bind(now.to_rfc3339())
                        .bind(new_status)
                        .bind(error_clone.as_ref())
                        .bind(success.then(|| now.to_rfc3339()))
                        .bind(provider_id)
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }
                DatabasePool::Postgres(p) => {
                    let update_sql = "UPDATE providers SET last_fetch_at = $1, last_fetch_status = $2, last_fetch_error = $3, last_success_at = COALESCE($4, last_success_at) WHERE id = $5";
                    sqlx::query(update_sql)
                        .bind(now)
                        .bind(new_status)
                        .bind(error_clone.as_ref())
                        .bind(success.then_some(now))
                        .bind(provider_id)
                        .execute(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }
            }

            Ok(status_changed)
        }).await
    }

    pub async fn get_provider_fetch_status(
        &self, provider_id: i64,
    ) -> DomainResult<ProviderFetchStatus> {
        let sql = format!(
            "SELECT last_fetch_status, last_fetch_error, last_fetch_at, last_success_at FROM providers WHERE id = {}",
            self.placeholder(1)
        );
        let not_found = |_| DomainError::ProviderNotFound(provider_id.to_string());
        let (row_status, error, fetched_at, succeeded_at) = match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(provider_id)
                    .fetch_one(p)
                    .await
                    .map_err(not_found)?;
                let time = |index: usize| {
                    row.try_get::<Option<String>, _>(index)
                        .ok()
                        .flatten()
                        .and_then(|at| DateTime::parse_from_rfc3339(&at).ok())
                        .map(|at| at.with_timezone(&Utc))
                };

                (
                    row.try_get::<String, _>(0).ok(),
                    row.try_get(1).ok().flatten(),
                    time(2),
                    time(3),
                )
            }
            DatabasePool::Postgres(p) => {
                let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(provider_id)
                    .fetch_one(p)
                    .await
                    .map_err(not_found)?;

                (
                    row.try_get::<String, _>(0).ok(),
                    row.try_get(1).ok().flatten(),
                    row.try_get(2).ok().flatten(),
                    row.try_get(3).ok().flatten(),
                )
            }
        };

        Ok(ProviderFetchStatus {
            status: row_status.unwrap_or_else(|| FETCH_STATUS_NEVER.to_string()),
            error,
            fetched_at,
            succeeded_at,
        })
    }

    pub async fn reset_provider_fetch_status(&self, provider_id: i64) -> DomainResult<()> {
        let cache_pool = self.cache_pool.clone();

        retry_on_busy(|| async {
            match &cache_pool {
                DatabasePool::Sqlite(p) => {
                    sqlx::query(
                        "UPDATE providers SET last_fetch_at = NULL, last_success_at = NULL, last_fetch_status = ?, last_fetch_error = NULL WHERE id = ?",
                    )
                    .bind(FETCH_STATUS_NEVER)
                    .bind(provider_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }
                DatabasePool::Postgres(p) => {
                    sqlx::query(
                        "UPDATE providers SET last_fetch_at = NULL, last_success_at = NULL, last_fetch_status = $1, last_fetch_error = NULL WHERE id = $2",
                    )
                    .bind(FETCH_STATUS_NEVER)
                    .bind(provider_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }
            }

            Ok(())
        })
        .await
    }

    pub async fn clear_provider_run_history(&self, provider_id: i64) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM run_history_cache WHERE pipeline_id IN (SELECT id FROM pipelines_cache WHERE provider_id = {})",
            self.placeholder(1)
        );
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(provider_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(provider_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        if let Some(pid) = provider_id {
            let sql = format!(
                "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type, metadata_json, resource_state_json
                FROM pipelines_cache
                WHERE provider_id = {}
                ORDER BY last_updated DESC",
                self.placeholder(1)
            );
            match &self.cache_pool {
                DatabasePool::Sqlite(p) => {
                    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                        .bind(pid)
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    rows.iter().map(|row| self.pipeline_from_row(row)).collect()
                }
                DatabasePool::Postgres(p) => {
                    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                        .bind(pid)
                        .fetch_all(p)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    rows.iter().map(|row| self.pipeline_from_row(row)).collect()
                }
            }
        } else {
            let sql = "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type, metadata_json, resource_state_json
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
            self.placeholder(1)
        );
        let result = match &self.cache_pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
                .bind(workflow_id)
                .fetch_optional(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
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
                            let prepared_data = chunk
                                .iter()
                                .map(run_cache_row)
                                .collect::<DomainResult<Vec<_>>>()?;

                            let values_clause = prepared_data
                                .iter()
                                .map(|_| "(?, ?, ?, datetime('now'), ?, ?)")
                                .collect::<Vec<_>>()
                                .join(", ");

                            let sql = format!(
                                "INSERT OR REPLACE INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash, started_at_ms) VALUES {}",
                                values_clause
                            );

                            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                            for (run_number, run_data, run_hash, started_at_ms) in &prepared_data {
                                query = query
                                    .bind(&pipeline_id_clone)
                                    .bind(run_number)
                                    .bind(run_data)
                                    .bind(run_hash)
                                    .bind(started_at_ms);
                            }

                            query.execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        }

                        tx.commit().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                    DatabasePool::Postgres(p) => {
                        let mut tx = p.begin().await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                        for chunk in runs_clone.chunks(BATCH_SIZE) {
                            let prepared_data = chunk
                                .iter()
                                .map(run_cache_row)
                                .collect::<DomainResult<Vec<_>>>()?;

                            let mut param_idx = 0;
                            let values_clauses: Vec<String> = prepared_data
                                .iter()
                                .map(|_| {
                                    let clause = format!(
                                        "(${}, ${}, ${}, NOW(), ${}, ${})",
                                        param_idx + 1, param_idx + 2, param_idx + 3, param_idx + 4, param_idx + 5
                                    );
                                    param_idx += 5;
                                    clause
                                })
                                .collect();

                            let sql = format!(
                                "INSERT INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash, started_at_ms) VALUES {} ON CONFLICT (pipeline_id, run_number) DO UPDATE SET run_data = EXCLUDED.run_data, fetched_at = EXCLUDED.fetched_at, run_hash = EXCLUDED.run_hash, started_at_ms = EXCLUDED.started_at_ms",
                                values_clauses.join(", ")
                            );

                            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                            for (run_number, run_data, run_hash, started_at_ms) in &prepared_data {
                                query = query
                                    .bind(&pipeline_id_clone)
                                    .bind(run_number)
                                    .bind(run_data)
                                    .bind(run_hash)
                                    .bind(started_at_ms);
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
             ORDER BY {}
             LIMIT {}",
            self.placeholder(1),
            run_order(pipeline_id),
            self.placeholder(2)
        );
        let rows = match &self.cache_pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
                .bind(pipeline_id)
                .bind(limit as i64)
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
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

    pub async fn get_cached_run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Option<PipelineRun>> {
        let sql = format!(
            "SELECT run_data FROM run_history_cache WHERE pipeline_id = {} AND run_number = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let row = match &self.cache_pool {
            DatabasePool::Sqlite(pool) => {
                sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
                    .bind(pipeline_id)
                    .bind(run_number)
                    .fetch_optional(pool)
                    .await
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
                    .bind(pipeline_id)
                    .bind(run_number)
                    .fetch_optional(pool)
                    .await
            }
        }
        .map_err(|error| DomainError::DatabaseError(error.to_string()))?;

        row.map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|error| DomainError::DatabaseError(format!("Invalid cached run: {error}")))
    }

    pub async fn clear_cached_run_history(&self, pipeline_id: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM run_history_cache WHERE pipeline_id = {}",
            self.placeholder(1)
        );
        match &self.cache_pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(pipeline_id)
                    .execute(p)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            }
            DatabasePool::Postgres(p) => {
                sqlx::query(sqlx::AssertSqlSafe(sql))
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
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
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
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
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
                                let prepared_data = chunk
                                    .iter()
                                    .copied()
                                    .map(run_cache_row)
                                    .collect::<DomainResult<Vec<_>>>()?;

                                let values_clause = prepared_data
                                    .iter()
                                    .map(|_| "(?, ?, ?, datetime('now'), ?, ?)")
                                    .collect::<Vec<_>>()
                                    .join(", ");

                                let sql = format!(
                                    "INSERT OR REPLACE INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash, started_at_ms) VALUES {}",
                                    values_clause
                                );

                                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                                for (run_number, run_data, run_hash, started_at_ms) in &prepared_data {
                                    query = query
                                        .bind(&pipeline_id_clone)
                                        .bind(run_number)
                                        .bind(run_data)
                                        .bind(run_hash)
                                        .bind(started_at_ms);
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

                                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(&pipeline_id_clone);
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
                                let prepared_data = chunk
                                    .iter()
                                    .copied()
                                    .map(run_cache_row)
                                    .collect::<DomainResult<Vec<_>>>()?;

                                let mut param_idx = 0;
                                let values_clauses: Vec<String> = prepared_data
                                    .iter()
                                    .map(|_| {
                                        let clause = format!(
                                            "(${}, ${}, ${}, NOW(), ${}, ${})",
                                            param_idx + 1, param_idx + 2, param_idx + 3, param_idx + 4, param_idx + 5
                                        );
                                        param_idx += 5;
                                        clause
                                    })
                                    .collect();

                                let sql = format!(
                                    "INSERT INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash, started_at_ms) VALUES {} ON CONFLICT (pipeline_id, run_number) DO UPDATE SET run_data = EXCLUDED.run_data, fetched_at = EXCLUDED.fetched_at, run_hash = EXCLUDED.run_hash, started_at_ms = EXCLUDED.started_at_ms",
                                    values_clauses.join(", ")
                                );

                                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
                                for (run_number, run_data, run_hash, started_at_ms) in &prepared_data {
                                    query = query
                                        .bind(&pipeline_id_clone)
                                        .bind(run_number)
                                        .bind(run_data)
                                        .bind(run_hash)
                                        .bind(started_at_ms);
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

                                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
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
                    "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type, metadata_json, resource_state_json
                    FROM pipelines_cache
                    WHERE provider_id = $1"
                } else {
                    "SELECT id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, provider_type, metadata_json, resource_state_json
                    FROM pipelines_cache
                    WHERE provider_id = ?"
                };

                let update_sql = if is_pg {
                    "UPDATE pipelines_cache
                     SET provider_id = $1, name = $2, status = $3, repository = $4, branch = $5,
                         workflow_file = $6, last_run = $7, last_updated = $8, metadata_json = $9, provider_type = $10,
                         resource_state_json = $11
                     WHERE id = $12"
                } else {
                    "UPDATE pipelines_cache
                     SET provider_id = ?, name = ?, status = ?, repository = ?, branch = ?,
                         workflow_file = ?, last_run = ?, last_updated = ?, metadata_json = ?, provider_type = ?,
                         resource_state_json = ?
                     WHERE id = ?"
                };

                let insert_sql = if is_pg {
                    "INSERT INTO pipelines_cache
                     (id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, metadata_json, provider_type, resource_state_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
                } else {
                    "INSERT INTO pipelines_cache
                     (id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, metadata_json, provider_type, resource_state_json)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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

                        let existing: HashMap<String, Pipeline> = existing_rows.iter()
                            .map(|row| {
                                let pipeline = self.pipeline_from_row(row)?;
                                Ok((pipeline.id.clone(), pipeline))
                            })
                            .collect::<DomainResult<_>>()?;

                        let new_ids: HashMap<String, &Pipeline> =
                            new_pipelines_clone.iter().map(|p| (p.id.clone(), p)).collect();

                        for pipeline in &new_pipelines_clone {
                            if let Some(old) = existing.get(&pipeline.id) {
                                if old.status != pipeline.status
                                    || old.last_run != pipeline.last_run
                                    || old.name != pipeline.name
                                    || old.provider_id != provider_id
                                    || old.provider_type != pipeline.provider_type
                                    || old.repository != pipeline.repository
                                    || old.branch != pipeline.branch
                                    || old.workflow_file != pipeline.workflow_file
                                    || old.metadata != pipeline.metadata
                                    || old.resource_state != pipeline.resource_state
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
                                        .bind(serde_json::to_string(&pipeline.metadata).map_err(|e| DomainError::DatabaseError(e.to_string()))?)
                                        .bind(&pipeline.provider_type)
                                        .bind(resource_state_json(pipeline)?)
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
                                    .bind(serde_json::to_string(&pipeline.metadata).map_err(|e| DomainError::DatabaseError(e.to_string()))?)
                                    .bind(&pipeline.provider_type)
                                    .bind(resource_state_json(pipeline)?)
                                    .execute(&mut *tx)
                                    .await
                                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        for old_id in existing.keys() {
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

                        let existing: HashMap<String, Pipeline> = existing_rows.iter()
                            .map(|row| {
                                let pipeline = self.pipeline_from_row(row)?;
                                Ok((pipeline.id.clone(), pipeline))
                            })
                            .collect::<DomainResult<_>>()?;

                        let new_ids: HashMap<String, &Pipeline> = new_pipelines_clone.iter().map(|p| (p.id.clone(), p)).collect();

                        for pipeline in &new_pipelines_clone {
                            let now = Utc::now();
                            if let Some(old) = existing.get(&pipeline.id) {
                                if old.status != pipeline.status || old.last_run != pipeline.last_run || old.name != pipeline.name || old.provider_id != provider_id || old.provider_type != pipeline.provider_type
                                    || old.repository != pipeline.repository || old.branch != pipeline.branch || old.workflow_file != pipeline.workflow_file || old.metadata != pipeline.metadata
                                    || old.resource_state != pipeline.resource_state {
                                    sqlx::query(update_sql)
                                        .bind(provider_id).bind(&pipeline.name).bind(pipeline.status.as_str()).bind(&pipeline.repository).bind(&pipeline.branch)
                                        .bind(&pipeline.workflow_file).bind(pipeline.last_run).bind(now)
                                        .bind(serde_json::to_string(&pipeline.metadata).map_err(|e| DomainError::DatabaseError(e.to_string()))?)
                                        .bind(&pipeline.provider_type).bind(resource_state_json(pipeline)?).bind(&pipeline.id)
                                        .execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                                }
                            } else {
                                sqlx::query(insert_sql)
                                    .bind(&pipeline.id).bind(provider_id).bind(&pipeline.name).bind(pipeline.status.as_str()).bind(&pipeline.repository).bind(&pipeline.branch)
                                    .bind(&pipeline.workflow_file).bind(pipeline.last_run).bind(now)
                                    .bind(serde_json::to_string(&pipeline.metadata).map_err(|e| DomainError::DatabaseError(e.to_string()))?)
                                    .bind(&pipeline.provider_type).bind(resource_state_json(pipeline)?)
                                    .execute(&mut *tx).await.map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                            }
                        }

                        for old_id in existing.keys() {
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
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
                .bind(pipeline_id)
                .fetch_one(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
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
             ORDER BY {}
             LIMIT {} OFFSET {}",
            self.placeholder(1),
            run_order(pipeline_id),
            self.placeholder(2),
            self.placeholder(3)
        );
        let runs_json = match &self.cache_pool {
            DatabasePool::Sqlite(p) => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
                .bind(pipeline_id)
                .bind(page_size as i64)
                .bind(offset as i64)
                .fetch_all(p)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?,
            DatabasePool::Postgres(p) => sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
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
        let metadata_json: String = row
            .try_get(10)
            .map_err(|error| DomainError::DatabaseError(error.to_string()))?;
        let metadata = serde_json::from_str(&metadata_json).map_err(|error| {
            DomainError::DatabaseError(format!("Invalid pipeline metadata: {error}"))
        })?;
        let resource_state = row
            .try_get::<Option<String>, _>(11)
            .map_err(|error| DomainError::DatabaseError(error.to_string()))?
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|error| {
                DomainError::DatabaseError(format!("Invalid pipeline resource state: {error}"))
            })?;

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
            resource_state,
            metadata,
            overdue_run: None,
        })
    }
}

fn resource_state_json(pipeline: &Pipeline) -> DomainResult<Option<String>> {
    pipeline
        .resource_state
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| DomainError::DatabaseError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::ResourceState;
    use crate::infrastructure::{
        MemoryTokenStore,
        PipedashConfig,
        StorageManager,
    };

    async fn test_repository(dir: &tempfile::TempDir) -> (StorageManager, Repository) {
        let mut config = PipedashConfig::default();
        config.storage.data_dir = dir.path().display().to_string();
        let manager =
            StorageManager::with_token_store(config, Arc::new(MemoryTokenStore::new()), true)
                .await
                .unwrap();
        let repository = Repository::new(
            manager.config_backend(),
            manager.database_pool(),
            manager.token_store().await,
        );

        (manager, repository)
    }

    fn cached_run(pipeline_id: &str, run_number: i64, started_at: &str) -> PipelineRun {
        serde_json::from_value(json!({
            "id": format!("{pipeline_id}__{run_number}"), "pipeline_id": pipeline_id,
            "run_number": run_number, "status": "success", "started_at": started_at,
            "logs_url": "", "metadata": {}
        }))
        .unwrap()
    }

    async fn run_pages(repository: &Repository, pipeline_id: &str) -> Vec<Vec<i64>> {
        let mut pages = Vec::new();
        for page in 1..=3 {
            let runs = repository
                .get_paginated_runs(pipeline_id, page, 2)
                .await
                .unwrap();
            pages.push(runs.iter().map(|run| run.run_number).collect());
        }

        pages
    }

    #[tokio::test]
    async fn pipeline_cache_keeps_resource_state_and_stores_state_only_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, repository) = test_repository(&dir).await;
        let provider_id = manager
            .config_backend()
            .create_provider(&ProviderConfig {
                id: None,
                name: "Flux".to_string(),
                provider_type: "fluxcd".to_string(),
                token: String::new(),
                config: HashMap::new(),
                refresh_interval: 30,
                version: None,
            })
            .await
            .unwrap();
        let mut pipeline: Pipeline = serde_json::from_value(json!({
            "id": "fluxcd__1__Kustomization__apps__web", "provider_id": provider_id,
            "provider_type": "fluxcd", "name": "Kustomization/web", "status": "running",
            "repository": "apps/source", "last_updated": "2026-10-21T00:00:00Z",
            "resource_state": { "label": "Reconciling", "message": "Applying revision a" },
            "metadata": { "uid": "resource-1" }
        }))
        .unwrap();
        let mut stateless = pipeline.clone();
        stateless.id = "fluxcd__1__Kustomization__apps__api".to_string();
        stateless.resource_state = None;

        repository
            .update_pipelines_cache(provider_id, &[pipeline.clone(), stateless])
            .await
            .unwrap();
        let first = repository
            .get_cached_pipelines(Some(provider_id))
            .await
            .unwrap();
        let stored = |pipelines: &[Pipeline], id: &str| {
            pipelines
                .iter()
                .find(|candidate| candidate.id == id)
                .cloned()
                .unwrap()
        };
        assert_eq!(
            stored(&first, &pipeline.id).resource_state,
            pipeline.resource_state
        );
        assert_eq!(stored(&first, &pipeline.id).metadata, pipeline.metadata);
        assert_eq!(
            stored(&first, "fluxcd__1__Kustomization__apps__api").resource_state,
            None
        );

        pipeline.resource_state = Some(ResourceState::new(
            "Reconciling",
            Some("Applying revision b"),
        ));
        repository
            .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
            .await
            .unwrap();
        let changed = repository.get_cached_pipelines(None).await.unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].resource_state, pipeline.resource_state);
        assert_eq!(changed[0].status, pipeline.status);
    }

    fn jenkins_provider() -> ProviderConfig {
        ProviderConfig {
            id: None,
            name: "ci-jenkins".to_string(),
            provider_type: "jenkins".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 60,
            version: None,
        }
    }

    #[tokio::test]
    async fn get_provider_reads_a_missing_token_entry_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, repository) = test_repository(&dir).await;
        let id = manager
            .config_backend()
            .create_provider(&jenkins_provider())
            .await
            .unwrap();

        let provider = repository.get_provider(id).await.unwrap();

        assert_eq!(provider.name, "ci-jenkins");
        assert_eq!(provider.token, "");
    }

    struct FailingTokenStore;

    #[async_trait::async_trait]
    impl TokenStore for FailingTokenStore {
        async fn store_token(&self, _provider_id: i64, _token: &str) -> DomainResult<()> {
            Ok(())
        }

        async fn get_token(&self, _provider_id: i64) -> DomainResult<String> {
            Err(DomainError::AuthenticationFailed(
                "vault is locked".to_string(),
            ))
        }

        async fn delete_token(&self, _provider_id: i64) -> DomainResult<()> {
            Ok(())
        }

        async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
            Ok(HashMap::new())
        }
    }

    #[tokio::test]
    async fn get_provider_fails_when_the_token_store_fails() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, _) = test_repository(&dir).await;
        let id = manager
            .config_backend()
            .create_provider(&jenkins_provider())
            .await
            .unwrap();
        let repository = Repository::new(
            manager.config_backend(),
            manager.database_pool(),
            Arc::new(FailingTokenStore),
        );

        let result = repository.get_provider(id).await;

        assert!(matches!(result, Err(DomainError::AuthenticationFailed(_))));
    }

    #[tokio::test]
    async fn native_run_cache_lists_the_latest_activity_first_on_every_page() {
        let dir = tempfile::tempdir().unwrap();
        let (_manager, repository) = test_repository(&dir).await;
        let flux = "fluxcd__1__Kustomization__apps__web";
        let other = "github__1__acme__web";
        let runs = |pipeline_id: &str| {
            [
                cached_run(pipeline_id, 100, "2026-10-22T10:00:00Z"),
                cached_run(pipeline_id, 200, "2026-10-20T10:00:00Z"),
                cached_run(pipeline_id, 300, "2026-10-21T10:00:00.500Z"),
                cached_run(pipeline_id, 400, "2026-10-21T10:00:00Z"),
                cached_run(pipeline_id, 500, "2026-10-21T10:00:00Z"),
            ]
        };
        repository
            .cache_run_history(flux, &runs(flux))
            .await
            .unwrap();
        repository
            .cache_run_history(other, &runs(other))
            .await
            .unwrap();

        assert_eq!(
            run_pages(&repository, flux).await,
            vec![vec![100, 300], vec![500, 400], vec![200]]
        );
        assert_eq!(
            run_pages(&repository, other).await,
            vec![vec![500, 400], vec![300, 200], vec![100]]
        );

        let mut reconciled_again = runs(flux)[1].clone();
        reconciled_again.started_at = "2026-10-23T10:00:00Z".parse().unwrap();
        let mut archived = runs(flux)[0].clone();
        archived
            .metadata
            .insert("archived".to_string(), json!(true));
        repository
            .merge_run_cache(
                flux,
                Vec::new(),
                vec![reconciled_again, archived],
                Vec::new(),
            )
            .await
            .unwrap();

        assert_eq!(
            run_pages(&repository, flux).await,
            vec![vec![200, 100], vec![300, 500], vec![400]]
        );
        let newest = repository.get_cached_run_history(flux, 3).await.unwrap();
        assert_eq!(
            newest.iter().map(|run| run.run_number).collect::<Vec<_>>(),
            vec![200, 100, 300]
        );
        let kept = repository.get_cached_run(flux, 100).await.unwrap().unwrap();
        assert_eq!(kept.metadata["archived"], json!(true));
        assert_eq!(repository.get_cached_run_count(flux).await.unwrap(), 5);
    }

    #[tokio::test]
    async fn sqlite_migration_backfills_the_order_key_of_cached_flux_runs() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE run_history_cache (
                pipeline_id TEXT NOT NULL,
                run_number INTEGER NOT NULL,
                run_data TEXT NOT NULL,
                PRIMARY KEY (pipeline_id, run_number)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        let timestamps = [
            "2026-10-21T10:00:00Z",
            "2026-10-21T10:00:00.1Z",
            "2026-10-21T10:00:00.500999Z",
            "2026-10-21T10:00:00.999999Z",
        ];
        let pipelines = [
            "fluxcd__1__Kustomization__apps__web",
            "github__1__acme__web",
        ];
        for pipeline_id in pipelines {
            for (index, timestamp) in timestamps.iter().enumerate() {
                sqlx::query("INSERT INTO run_history_cache (pipeline_id, run_number, run_data) VALUES (?, ?, ?)")
                    .bind(pipeline_id)
                    .bind((index + 1) as i64)
                    .bind(json!({ "started_at": timestamp }).to_string())
                    .execute(&pool).await.unwrap();
            }
        }

        sqlx::raw_sql(include_str!(
            "../../../migrations/sqlite/20261021000001_run_history_started_at.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let keys: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT pipeline_id, run_number, started_at_ms FROM run_history_cache ORDER BY pipeline_id, run_number",
        ).fetch_all(&pool).await.unwrap();
        let expected: Vec<_> = pipelines
            .iter()
            .flat_map(|pipeline_id| {
                timestamps
                    .iter()
                    .enumerate()
                    .map(move |(index, timestamp)| {
                        let millis = if pipeline_id.starts_with("fluxcd__") {
                            timestamp
                                .parse::<DateTime<Utc>>()
                                .unwrap()
                                .timestamp_millis()
                        } else {
                            0
                        };
                        (pipeline_id.to_string(), (index + 1) as i64, millis)
                    })
            })
            .collect();
        assert_eq!(keys, expected);
    }
}
