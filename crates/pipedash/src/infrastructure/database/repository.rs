use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::{
    engine::general_purpose,
    Engine as _,
};
use chrono::{
    DateTime,
    Utc,
};
use keyring::Entry;
use sqlx::{
    Row as SqlxRow,
    SqlitePool,
};
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    ProviderConfig,
};

const FETCH_STATUS_SUCCESS: &str = "success";
const FETCH_STATUS_ERROR: &str = "error";
const FETCH_STATUS_NEVER: &str = "never";

async fn retry_on_busy<F, Fut, T>(operation: F) -> DomainResult<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = DomainResult<T>>,
{
    const MAX_RETRIES: u32 = 5;
    const INITIAL_DELAY_MS: u64 = 10;

    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(DomainError::DatabaseError(ref msg))
                if (msg.contains("database is locked")
                    || msg.contains("SQLITE_BUSY")
                    || msg.contains("locked"))
                    && attempt < MAX_RETRIES =>
            {
                attempt += 1;
                let delay = INITIAL_DELAY_MS * 2_u64.pow(attempt - 1);
                sleep(Duration::from_millis(delay)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

pub struct Repository {
    pool: SqlitePool,
    keyring_lock: Arc<Mutex<()>>,
    token_cache: Arc<Mutex<Option<HashMap<String, String>>>>,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            keyring_lock: Arc::new(Mutex::new(())),
            token_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn add_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM providers WHERE name = ?")
            .bind(&config.name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if existing.is_some() {
            return Err(DomainError::InvalidConfig(format!(
                "A provider with the name '{}' already exists",
                config.name
            )));
        }

        let config_json = serde_json::to_string(&config.config)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let result = sqlx::query(
            "INSERT INTO providers (name, provider_type, token_encrypted, config_json, refresh_interval) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&config.name)
        .bind(&config.provider_type)
        .bind("")
        .bind(&config_json)
        .bind(config.refresh_interval)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let provider_id = result.last_insert_rowid();

        self.store_token_in_keyring(provider_id, &config.token)?;

        Ok(provider_id)
    }

    pub async fn get_provider(&self, id: i64) -> DomainResult<ProviderConfig> {
        let row = sqlx::query(
            "SELECT id, name, provider_type, token_encrypted, config_json, refresh_interval FROM providers WHERE id = ?"
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| DomainError::ProviderNotFound(id.to_string()))?;

        self.provider_from_row(&row)
    }

    pub async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            "SELECT id, name, provider_type, token_encrypted, config_json, refresh_interval FROM providers"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        // Process providers individually, logging errors but continuing
        let mut providers = Vec::new();
        for row in rows.iter() {
            match self.provider_from_row(row) {
                Ok(provider) => {
                    providers.push(provider);
                }
                Err(_e) => {
                    // Skip providers that fail to load
                }
            }
        }

        Ok(providers)
    }

    pub async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        let config_json = serde_json::to_string(&config.config)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE providers SET name = ?, token_encrypted = ?, config_json = ?, refresh_interval = ? WHERE id = ?"
        )
        .bind(&config.name)
        .bind("")
        .bind(&config_json)
        .bind(config.refresh_interval)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::ProviderNotFound(id.to_string()));
        }

        self.store_token_in_keyring(id, &config.token)?;

        Ok(())
    }

    pub async fn remove_provider(&self, id: i64) -> DomainResult<()> {
        let pipelines = self
            .get_cached_pipelines(Some(id))
            .await
            .unwrap_or_default();
        let pipeline_ids: Vec<String> = pipelines.iter().map(|p| p.id.clone()).collect();

        let _ = sqlx::query("DELETE FROM run_history_cache WHERE pipeline_id IN (SELECT id FROM pipelines_cache WHERE provider_id = ?)")
            .bind(id)
            .execute(&self.pool)
            .await;

        for pipeline_id in pipeline_ids {
            let _ = sqlx::query("DELETE FROM workflow_parameters_cache WHERE workflow_id LIKE ?")
                .bind(format!("{}%", pipeline_id))
                .execute(&self.pool)
                .await;
        }

        let result = sqlx::query("DELETE FROM providers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::ProviderNotFound(id.to_string()));
        }

        let _ = self.delete_token_from_keyring(id);

        Ok(())
    }

    pub async fn update_provider_fetch_status(
        &self, provider_id: i64, success: bool, error: Option<String>,
    ) -> DomainResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let status = if success {
            FETCH_STATUS_SUCCESS
        } else {
            FETCH_STATUS_ERROR
        };

        sqlx::query(
            "UPDATE providers SET last_fetch_at = ?, last_fetch_status = ?, last_fetch_error = ? WHERE id = ?"
        )
        .bind(&now)
        .bind(status)
        .bind(error.as_ref())
        .bind(provider_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_provider_fetch_status(
        &self, provider_id: i64,
    ) -> DomainResult<(String, Option<String>, Option<String>)> {
        let row = sqlx::query(
            "SELECT last_fetch_status, last_fetch_error, last_fetch_at FROM providers WHERE id = ?",
        )
        .bind(provider_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_e| DomainError::ProviderNotFound(provider_id.to_string()))?;

        let status: String = row
            .try_get(0)
            .unwrap_or_else(|_| FETCH_STATUS_NEVER.to_string());
        let error: Option<String> = row.try_get(1).ok().flatten();
        let fetch_at: Option<String> = row.try_get(2).ok().flatten();

        Ok((status, error, fetch_at))
    }

    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        let rows = if let Some(pid) = provider_id {
            sqlx::query(
                "SELECT pc.id, pc.provider_id, pc.name, pc.status, pc.repository, pc.branch, pc.workflow_file, pc.last_run, pc.last_updated, p.provider_type
                FROM pipelines_cache pc
                JOIN providers p ON pc.provider_id = p.id
                WHERE pc.provider_id = ?"
            )
            .bind(pid)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT pc.id, pc.provider_id, pc.name, pc.status, pc.repository, pc.branch, pc.workflow_file, pc.last_run, pc.last_updated, p.provider_type
                FROM pipelines_cache pc
                JOIN providers p ON pc.provider_id = p.id"
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?
        };

        rows.iter().map(|row| self.pipeline_from_row(row)).collect()
    }

    fn provider_from_row(&self, row: &sqlx::sqlite::SqliteRow) -> DomainResult<ProviderConfig> {
        let id: i64 = row
            .try_get(0)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let name: String = row
            .try_get(1)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let provider_type: String = row
            .try_get(2)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let token_encrypted: String = row
            .try_get(3)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let config_json: String = row
            .try_get(4)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let refresh_interval: i64 = row
            .try_get(5)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let token = if !token_encrypted.is_empty() {
            let decoded = general_purpose::STANDARD
                .decode(&token_encrypted)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            let token = String::from_utf8(decoded)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            let _ = self.store_token_in_keyring(id, &token);

            token
        } else {
            match self.get_token_from_keyring(id) {
                Ok(token) => token,
                Err(e) => {
                    return Err(DomainError::DatabaseError(format!(
                        "Failed to get token from keyring for provider {}: {}",
                        id, e
                    )));
                }
            }
        };

        let config: HashMap<String, String> = if config_json.trim().is_empty() {
            HashMap::new()
        } else {
            serde_json::from_str(&config_json).map_err(|e| {
                DomainError::DatabaseError(format!(
                    "Failed to parse config JSON '{}': {}",
                    config_json, e
                ))
            })?
        };

        Ok(ProviderConfig {
            id: Some(id),
            name,
            provider_type,
            token,
            config,
            refresh_interval,
        })
    }

    fn pipeline_from_row(&self, row: &sqlx::sqlite::SqliteRow) -> DomainResult<Pipeline> {
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

    fn keyring_entry(&self) -> DomainResult<Entry> {
        Entry::new("pipedash", "tokens")
            .map_err(|e| DomainError::DatabaseError(format!("Failed to create keyring entry: {e}")))
    }

    fn get_all_tokens(&self) -> DomainResult<HashMap<String, String>> {
        let mut cache = futures::executor::block_on(self.token_cache.lock());

        if let Some(ref cached_tokens) = *cache {
            return Ok(cached_tokens.clone());
        }

        let entry = self.keyring_entry()?;
        let tokens = match entry.get_password() {
            Ok(json) => {
                if json.trim().is_empty() {
                    HashMap::new()
                } else {
                    serde_json::from_str(&json).map_err(|e| {
                        DomainError::DatabaseError(format!("Failed to parse tokens JSON: {e}"))
                    })?
                }
            }
            Err(keyring::Error::NoEntry) => HashMap::new(),
            Err(e) => {
                return Err(DomainError::DatabaseError(format!(
                    "Failed to get tokens from keyring: {e}"
                )))
            }
        };

        *cache = Some(tokens.clone());
        Ok(tokens)
    }

    fn save_all_tokens(&self, tokens: &HashMap<String, String>) -> DomainResult<()> {
        let entry = self.keyring_entry()?;
        let json = serde_json::to_string(tokens)
            .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize tokens: {e}")))?;

        entry.set_password(&json).map_err(|e| {
            DomainError::DatabaseError(format!(
                "Failed to store tokens in system keyring: {}\n\
                 \nThe tokens will not be saved securely. Please ensure:\n\
                 - macOS: Grant Keychain Access permission to Pipedash\n\
                 - Linux: Install libsecret (sudo apt install libsecret-1-dev)\n\
                 - Windows: Ensure Credential Manager is accessible",
                e
            ))
        })?;

        // Update cache
        let mut cache = futures::executor::block_on(self.token_cache.lock());
        *cache = Some(tokens.clone());

        Ok(())
    }

    fn store_token_in_keyring(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let _lock = futures::executor::block_on(self.keyring_lock.lock());

        let mut tokens = self.get_all_tokens()?;
        tokens.insert(provider_id.to_string(), token.to_string());
        self.save_all_tokens(&tokens)
    }

    fn get_token_from_keyring(&self, provider_id: i64) -> DomainResult<String> {
        let _lock = futures::executor::block_on(self.keyring_lock.lock());

        let mut tokens = self.get_all_tokens()?;

        if let Some(token) = tokens.get(&provider_id.to_string()) {
            return Ok(token.clone());
        }

        let old_entry =
            Entry::new("pipedash", &format!("provider_{}", provider_id)).map_err(|e| {
                DomainError::DatabaseError(format!("Failed to create old keyring entry: {e}"))
            })?;

        if let Ok(token) = old_entry.get_password() {
            tokens.insert(provider_id.to_string(), token.clone());
            self.save_all_tokens(&tokens)?;

            let _ = old_entry.delete_credential();

            return Ok(token);
        }

        Err(DomainError::DatabaseError(format!(
            "Token not found in keyring for provider {}",
            provider_id
        )))
    }

    fn delete_token_from_keyring(&self, provider_id: i64) -> DomainResult<()> {
        let _lock = futures::executor::block_on(self.keyring_lock.lock());

        let mut tokens = self.get_all_tokens()?;
        tokens.remove(&provider_id.to_string());

        if tokens.is_empty() {
            let entry = self.keyring_entry()?;
            entry.delete_credential().map_err(|e| {
                DomainError::DatabaseError(format!("Failed to delete keyring entry: {e}"))
            })
        } else {
            self.save_all_tokens(&tokens)
        }
    }

    pub async fn cache_workflow_parameters(
        &self, workflow_id: &str, parameters: &[pipedash_plugin_api::WorkflowParameter],
    ) -> DomainResult<()> {
        let parameters_json = serde_json::to_string(parameters)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        sqlx::query(
            "INSERT OR REPLACE INTO workflow_parameters_cache (workflow_id, parameters_json, cached_at)
             VALUES (?, ?, datetime('now'))"
        )
        .bind(workflow_id)
        .bind(&parameters_json)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_cached_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Option<Vec<pipedash_plugin_api::WorkflowParameter>>> {
        let result = sqlx::query_scalar::<_, String>(
            "SELECT parameters_json FROM workflow_parameters_cache WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

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
        sqlx::query("DELETE FROM workflow_parameters_cache")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn cache_run_history(
        &self, pipeline_id: &str, runs: &[PipelineRun],
    ) -> DomainResult<()> {
        if runs.is_empty() {
            return Ok(());
        }

        let pipeline_id_str = pipeline_id.to_string();
        let runs_vec = runs.to_vec();
        let pool = self.pool.clone();

        retry_on_busy(|| {
            let pipeline_id_clone = pipeline_id_str.clone();
            let runs_clone = runs_vec.clone();
            let pool_clone = pool.clone();
            async move {
                let mut tx = pool_clone.begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for run in &runs_clone {
                    let run_data = serde_json::to_string(run)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    let status_str = match run.status {
                        crate::domain::PipelineStatus::Success => "success",
                        crate::domain::PipelineStatus::Failed => "failed",
                        crate::domain::PipelineStatus::Running => "running",
                        crate::domain::PipelineStatus::Pending => "pending",
                        crate::domain::PipelineStatus::Cancelled => "cancelled",
                        crate::domain::PipelineStatus::Skipped => "skipped",
                    };

                    let run_hash = crate::infrastructure::deduplication::hash_pipeline_run(
                        run.run_number,
                        status_str,
                        run.branch.as_deref(),
                        &run.started_at.to_rfc3339(),
                        run.duration_seconds,
                        run.commit_sha.as_deref(),
                    );

                    sqlx::query(
                        "INSERT OR REPLACE INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash)
                         VALUES (?, ?, ?, datetime('now'), ?)"
                    )
                    .bind(&pipeline_id_clone)
                    .bind(run.run_number)
                    .bind(&run_data)
                    .bind(&run_hash)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                Ok(())
            }
        })
        .await
    }

    pub async fn get_cached_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT run_data FROM run_history_cache
             WHERE pipeline_id = ?
             ORDER BY run_number DESC
             LIMIT ?",
        )
        .bind(pipeline_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let runs: Vec<PipelineRun> = rows
            .iter()
            .filter_map(|json| serde_json::from_str(json).ok())
            .collect();

        Ok(runs)
    }

    pub async fn clear_cached_run_history(&self, pipeline_id: &str) -> DomainResult<()> {
        sqlx::query("DELETE FROM run_history_cache WHERE pipeline_id = ?")
            .bind(pipeline_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn clear_all_run_history_cache(&self) -> DomainResult<()> {
        sqlx::query("DELETE FROM run_history_cache")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_cached_runs_with_hashes(
        &self, pipeline_id: &str,
    ) -> DomainResult<HashMap<i64, (PipelineRun, String)>> {
        let rows = sqlx::query(
            "SELECT run_number, run_data, run_hash FROM run_history_cache WHERE pipeline_id = ?",
        )
        .bind(pipeline_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let mut result = HashMap::new();
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

        Ok(result)
    }

    pub async fn merge_run_cache(
        &self, pipeline_id: &str, new_runs: Vec<PipelineRun>, changed_runs: Vec<PipelineRun>,
        deleted_run_numbers: Vec<i64>,
    ) -> DomainResult<()> {
        let pipeline_id_str = pipeline_id.to_string();
        let pool = self.pool.clone();

        retry_on_busy(move || {
            let pipeline_id_clone = pipeline_id_str.clone();
            let new_runs_clone = new_runs.clone();
            let changed_runs_clone = changed_runs.clone();
            let deleted_clone = deleted_run_numbers.clone();
            let pool_clone = pool.clone();

            async move {
                let mut tx = pool_clone
                    .begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                for run in new_runs_clone.iter().chain(changed_runs_clone.iter()) {
                    let run_data = serde_json::to_string(run)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    let status_str = run.status.as_str();
                    let run_hash = crate::infrastructure::deduplication::hash_pipeline_run(
                        run.run_number,
                        status_str,
                        run.branch.as_deref(),
                        &run.started_at.to_rfc3339(),
                        run.duration_seconds,
                        run.commit_sha.as_deref(),
                    );

                    sqlx::query(
                        "INSERT OR REPLACE INTO run_history_cache (pipeline_id, run_number, run_data, fetched_at, run_hash)
                         VALUES (?, ?, ?, datetime('now'), ?)"
                    )
                    .bind(&pipeline_id_clone)
                    .bind(run.run_number)
                    .bind(&run_data)
                    .bind(&run_hash)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }

                for run_number in &deleted_clone {
                    sqlx::query("DELETE FROM run_history_cache WHERE pipeline_id = ? AND run_number = ?")
                        .bind(&pipeline_id_clone)
                        .bind(run_number)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                Ok(())
            }
        })
        .await?;

        Ok(())
    }

    pub async fn update_pipelines_cache(
        &self, provider_id: i64, new_pipelines: &[Pipeline],
    ) -> DomainResult<()> {
        let new_pipelines_vec = new_pipelines.to_vec();
        let pool = self.pool.clone();

        retry_on_busy(move || {
            let new_pipelines_clone = new_pipelines_vec.clone();
            let pool_clone = pool.clone();
            async move {
                let mut tx = pool_clone.begin()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                let existing_rows = sqlx::query(
                    "SELECT pc.id, pc.provider_id, pc.name, pc.status, pc.repository, pc.branch, pc.workflow_file, pc.last_run, pc.last_updated, p.provider_type
                    FROM pipelines_cache pc
                    JOIN providers p ON pc.provider_id = p.id
                    WHERE pc.provider_id = ?"
                )
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
                    let last_run_str: Option<String> = row.try_get(7).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let last_updated_str: String = row.try_get(8).map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    let provider_type: String = row.try_get(9).map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                    let status: PipelineStatus = if status_str.trim().is_empty() {
                        PipelineStatus::Pending
                    } else if status_str.starts_with('"') {
                        serde_json::from_str(&status_str)
                            .map_err(|e| {
                                DomainError::DatabaseError(format!("Failed to parse status '{}': {}", status_str, e))
                            })?
                    } else {
                        let quoted = format!("\"{}\"", status_str);
                        serde_json::from_str(&quoted).map_err(|e| {
                            DomainError::DatabaseError(format!("Failed to parse status '{}': {}", status_str, e))
                        })?
                    };
                    let last_run = last_run_str
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc));
                    let last_updated = DateTime::parse_from_rfc3339(&last_updated_str)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                        .with_timezone(&Utc);

                    existing.insert(id.clone(), Pipeline {
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
                    });
                }

                let new_ids: HashMap<String, &Pipeline> =
                    new_pipelines_clone.iter().map(|p| (p.id.clone(), p)).collect();

                for pipeline in &new_pipelines_clone {
                    if let Some(old) = existing.get(&pipeline.id) {
                        if old.status != pipeline.status
                            || old.last_run != pipeline.last_run
                            || old.name != pipeline.name
                        {
                            sqlx::query(
                                "UPDATE pipelines_cache
                                 SET name = ?, status = ?, repository = ?, branch = ?,
                                     workflow_file = ?, last_run = ?, last_updated = ?, metadata_json = ?
                                 WHERE id = ?"
                            )
                            .bind(&pipeline.name)
                            .bind(pipeline.status.as_str())
                            .bind(&pipeline.repository)
                            .bind(&pipeline.branch)
                            .bind(&pipeline.workflow_file)
                            .bind(pipeline.last_run.as_ref().map(|dt| dt.to_rfc3339()))
                            .bind(Utc::now().to_rfc3339())
                            .bind("{}")
                            .bind(&pipeline.id)
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                        }
                    } else {
                        sqlx::query(
                            "INSERT INTO pipelines_cache
                             (id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, metadata_json)
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                        )
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
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                }

                for (old_id, _) in existing.iter() {
                    if !new_ids.contains_key(old_id) {
                        sqlx::query("DELETE FROM pipelines_cache WHERE id = ?")
                            .bind(old_id)
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                    }
                }

                tx.commit()
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                Ok(())
            }
        })
        .await
    }

    pub async fn get_pipelines_cache_count(&self) -> DomainResult<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pipelines_cache")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);
        Ok(count)
    }

    pub async fn get_run_history_cache_count(&self) -> DomainResult<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM run_history_cache")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);
        Ok(count)
    }

    pub async fn get_workflow_params_cache_count(&self) -> DomainResult<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workflow_parameters_cache")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);
        Ok(count)
    }

    pub async fn clear_pipelines_cache(&self) -> DomainResult<usize> {
        let result = sqlx::query("DELETE FROM pipelines_cache")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(result.rows_affected() as usize)
    }

    pub async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        let result = sqlx::query_scalar::<_, String>(
            "SELECT preferences_json FROM table_preferences WHERE provider_id = ? AND table_id = ?",
        )
        .bind(provider_id)
        .bind(table_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(result)
    }

    pub async fn upsert_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()> {
        retry_on_busy(|| {
            let provider_id_val = provider_id;
            let table_id_val = table_id.to_string();
            let preferences_val = preferences_json.to_string();
            let pool = self.pool.clone();

            async move {
                sqlx::query(
                    "INSERT INTO table_preferences (provider_id, table_id, preferences_json, created_at, updated_at)
                     VALUES (?, ?, ?, datetime('now'), datetime('now'))
                     ON CONFLICT(provider_id, table_id) DO UPDATE SET
                     preferences_json = excluded.preferences_json,
                     updated_at = datetime('now')"
                )
                .bind(provider_id_val)
                .bind(&table_id_val)
                .bind(&preferences_val)
                .execute(&pool)
                .await
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                Ok(())
            }
        })
        .await
    }

    /// Store provider permission status in the database
    pub async fn store_provider_permissions(
        &self, provider_id: i64, status: &pipedash_plugin_api::PermissionStatus,
    ) -> DomainResult<()> {
        retry_on_busy(|| {
            let provider_id_val = provider_id;
            let permissions = status.permissions.clone();
            let checked_at = status.checked_at.to_rfc3339();
            let pool = self.pool.clone();

            async move {
                // Delete existing permissions for this provider
                sqlx::query("DELETE FROM provider_permissions WHERE provider_id = ?")
                    .bind(provider_id_val)
                    .execute(&pool)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

                // Insert new permissions
                for perm_check in permissions {
                    sqlx::query(
                        "INSERT INTO provider_permissions (provider_id, permission_name, granted, checked_at)
                         VALUES (?, ?, ?, ?)"
                    )
                    .bind(provider_id_val)
                    .bind(&perm_check.permission.name)
                    .bind(perm_check.granted)
                    .bind(&checked_at)
                    .execute(&pool)
                    .await
                    .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                }

                Ok(())
            }
        })
        .await
    }

    /// Retrieve provider permission status from the database
    pub async fn get_provider_permissions(
        &self, provider_id: i64,
    ) -> DomainResult<Option<pipedash_plugin_api::PermissionStatus>> {
        let rows = sqlx::query(
            "SELECT permission_name, granted, checked_at
             FROM provider_permissions
             WHERE provider_id = ?
             ORDER BY permission_name",
        )
        .bind(provider_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if rows.is_empty() {
            return Ok(None);
        }

        // Get checked_at from first row (they're all the same)
        let checked_at_str: String = rows[0]
            .try_get("checked_at")
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        let checked_at = DateTime::parse_from_rfc3339(&checked_at_str)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            .with_timezone(&Utc);

        let mut permissions = Vec::new();
        let mut all_required_granted = true;

        for row in rows {
            let permission_name: String = row
                .try_get("permission_name")
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
            let granted: bool = row
                .try_get("granted")
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            // We don't have the full permission details (description, required)
            // stored in the database We create a minimal Permission object here
            // In practice, the application will use metadata from the plugin
            let permission = pipedash_plugin_api::Permission {
                name: permission_name,
                description: String::new(),
                required: false,
            };

            permissions.push(pipedash_plugin_api::PermissionCheck {
                permission,
                granted,
            });

            if !granted {
                all_required_granted = false;
            }
        }

        Ok(Some(pipedash_plugin_api::PermissionStatus {
            permissions,
            all_granted: all_required_granted,
            checked_at,
            metadata: std::collections::HashMap::new(),
        }))
    }
}
