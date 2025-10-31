use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
};

use base64::{
    engine::general_purpose,
    Engine as _,
};
use chrono::{
    DateTime,
    Utc,
};
use keyring::Entry;
use rusqlite::{
    params,
    Connection,
    Row,
};

use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineStatus,
    ProviderConfig,
};

pub struct Repository {
    conn: Arc<Mutex<Connection>>,
    keyring_lock: Arc<Mutex<()>>,
}

impl Repository {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            keyring_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn add_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let existing: Result<i64, _> = conn.query_row(
            "SELECT id FROM providers WHERE name = ?1",
            params![&config.name],
            |row| row.get(0),
        );

        if existing.is_ok() {
            return Err(DomainError::InvalidConfig(format!(
                "A provider with the name '{}' already exists",
                config.name
            )));
        }

        let config_json = serde_json::to_string(&config.config)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        conn.execute(
            "INSERT INTO providers (name, provider_type, token_encrypted, config_json, refresh_interval) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![config.name, &config.provider_type, "", config_json, config.refresh_interval],
        )
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let provider_id = conn.last_insert_rowid();

        self.store_token_in_keyring(provider_id, &config.token)?;

        Ok(provider_id)
    }

    pub fn get_provider(&self, id: i64) -> DomainResult<ProviderConfig> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        conn.query_row(
            "SELECT id, name, provider_type, token_encrypted, config_json, refresh_interval FROM providers WHERE id = ?1",
            params![id],
            |row| self.provider_from_row(row),
        )
        .map_err(|_| DomainError::ProviderNotFound(id.to_string()))
    }

    pub fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT id, name, provider_type, token_encrypted, config_json, refresh_interval FROM providers")
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let providers = stmt
            .query_map([], |row| self.provider_from_row(row))
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(providers)
    }

    pub fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let config_json = serde_json::to_string(&config.config)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let rows_affected = conn
            .execute(
                "UPDATE providers SET name = ?1, token_encrypted = ?2, config_json = ?3, refresh_interval = ?4 WHERE id = ?5",
                params![config.name, "", config_json, config.refresh_interval, id],
            )
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if rows_affected == 0 {
            return Err(DomainError::ProviderNotFound(id.to_string()));
        }

        self.store_token_in_keyring(id, &config.token)?;

        Ok(())
    }

    pub fn remove_provider(&self, id: i64) -> DomainResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let rows_affected = conn
            .execute("DELETE FROM providers WHERE id = ?1", params![id])
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if rows_affected == 0 {
            return Err(DomainError::ProviderNotFound(id.to_string()));
        }

        let _ = self.delete_token_from_keyring(id);

        Ok(())
    }

    pub fn cache_pipelines(&self, provider_id: i64, pipelines: &[Pipeline]) -> DomainResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        conn.execute(
            "DELETE FROM pipelines_cache WHERE provider_id = ?1",
            params![provider_id],
        )
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        for pipeline in pipelines {
            let last_run = pipeline.last_run.map(|dt| dt.to_rfc3339());
            let status_json = serde_json::to_string(&pipeline.status)
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            conn.execute(
                "INSERT INTO pipelines_cache
                (id, provider_id, name, status, repository, branch, workflow_file, last_run, last_updated, metadata_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    pipeline.id,
                    provider_id,
                    pipeline.name,
                    status_json,
                    pipeline.repository,
                    pipeline.branch,
                    pipeline.workflow_file,
                    last_run,
                    pipeline.last_updated.to_rfc3339(),
                    "{}",
                ],
            )
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
        }

        Ok(())
    }

    pub fn get_cached_pipelines(&self, provider_id: Option<i64>) -> DomainResult<Vec<Pipeline>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        if let Some(pid) = provider_id {
            let mut stmt = conn
                .prepare(
                    "SELECT pc.id, pc.provider_id, pc.name, pc.status, pc.repository, pc.branch, pc.workflow_file, pc.last_run, pc.last_updated, p.provider_type
                    FROM pipelines_cache pc
                    JOIN providers p ON pc.provider_id = p.id
                    WHERE pc.provider_id = ?1"
                )
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            let pipelines = stmt
                .query_map([pid], |row| self.pipeline_from_row(row))
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            Ok(pipelines)
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT pc.id, pc.provider_id, pc.name, pc.status, pc.repository, pc.branch, pc.workflow_file, pc.last_run, pc.last_updated, p.provider_type
                    FROM pipelines_cache pc
                    JOIN providers p ON pc.provider_id = p.id"
                )
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            let pipelines = stmt
                .query_map([], |row| self.pipeline_from_row(row))
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

            Ok(pipelines)
        }
    }

    fn provider_from_row(&self, row: &Row) -> rusqlite::Result<ProviderConfig> {
        let id: i64 = row.get(0)?;
        let name: String = row.get(1)?;
        let provider_type: String = row.get(2)?;
        let token_encrypted: String = row.get(3)?;
        let config_json: String = row.get(4)?;
        let refresh_interval: i64 = row.get(5)?;

        let token = if !token_encrypted.is_empty() {
            let decoded = general_purpose::STANDARD
                .decode(&token_encrypted)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            let token = String::from_utf8(decoded).map_err(|_| rusqlite::Error::InvalidQuery)?;

            if let Err(e) = self.store_token_in_keyring(id, &token) {
                eprintln!("[WARN] Could not migrate token {} to keyring: {}", id, e);
            } else {
                eprintln!("[INFO] Migrated token {} to keyring", id);
            }

            token
        } else {
            match self.get_token_from_keyring(id) {
                Ok(token) => token,
                Err(e) => {
                    eprintln!(
                        "[ERROR] Failed to get token from keyring for provider {}: {}",
                        id, e
                    );
                    return Err(rusqlite::Error::InvalidQuery);
                }
            }
        };

        let config: HashMap<String, String> =
            serde_json::from_str(&config_json).map_err(|_| rusqlite::Error::InvalidQuery)?;

        Ok(ProviderConfig {
            id: Some(id),
            name,
            provider_type,
            token,
            config,
            refresh_interval,
        })
    }

    fn pipeline_from_row(&self, row: &Row) -> rusqlite::Result<Pipeline> {
        let id: String = row.get(0)?;
        let provider_id: i64 = row.get(1)?;
        let name: String = row.get(2)?;
        let status_str: String = row.get(3)?;
        let repository: String = row.get(4)?;
        let branch: Option<String> = row.get(5)?;
        let workflow_file: Option<String> = row.get(6)?;
        let last_run_str: Option<String> = row.get(7)?;
        let last_updated_str: String = row.get(8)?;
        let provider_type: String = row.get(9)?;

        let status: PipelineStatus =
            serde_json::from_str(&status_str).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let last_run = last_run_str
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let last_updated = DateTime::parse_from_rfc3339(&last_updated_str)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
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
        })
    }

    fn keyring_entry(&self) -> DomainResult<Entry> {
        Entry::new("pipedash", "tokens")
            .map_err(|e| DomainError::DatabaseError(format!("Failed to create keyring entry: {e}")))
    }

    fn get_all_tokens(&self) -> DomainResult<HashMap<String, String>> {
        let entry = self.keyring_entry()?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str(&json).map_err(|e| {
                DomainError::DatabaseError(format!("Failed to parse tokens JSON: {e}"))
            }),
            Err(keyring::Error::NoEntry) => Ok(HashMap::new()),
            Err(e) => Err(DomainError::DatabaseError(format!(
                "Failed to get tokens from keyring: {e}"
            ))),
        }
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
        })
    }

    fn store_token_in_keyring(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let _lock = self.keyring_lock.lock().map_err(|e| {
            DomainError::DatabaseError(format!("Failed to acquire keyring lock: {}", e))
        })?;

        let mut tokens = self.get_all_tokens()?;
        tokens.insert(provider_id.to_string(), token.to_string());
        self.save_all_tokens(&tokens)
    }

    fn get_token_from_keyring(&self, provider_id: i64) -> DomainResult<String> {
        let _lock = self.keyring_lock.lock().map_err(|e| {
            DomainError::DatabaseError(format!("Failed to acquire keyring lock: {}", e))
        })?;

        let mut tokens = self.get_all_tokens()?;

        if let Some(token) = tokens.get(&provider_id.to_string()) {
            return Ok(token.clone());
        }

        let old_entry =
            Entry::new("pipedash", &format!("provider_{}", provider_id)).map_err(|e| {
                DomainError::DatabaseError(format!("Failed to create old keyring entry: {e}"))
            })?;

        if let Ok(token) = old_entry.get_password() {
            eprintln!(
                "[INFO] Migrating provider {} from old keyring format",
                provider_id
            );
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
        let _lock = self.keyring_lock.lock().map_err(|e| {
            DomainError::DatabaseError(format!("Failed to acquire keyring lock: {}", e))
        })?;

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

    pub fn cache_workflow_parameters(
        &self, workflow_id: &str, parameters: &[pipedash_plugin_api::WorkflowParameter],
    ) -> DomainResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let parameters_json = serde_json::to_string(parameters)
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO workflow_parameters_cache (workflow_id, parameters_json, cached_at)
             VALUES (?1, ?2, datetime('now'))",
            params![workflow_id, parameters_json],
        )
        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub fn get_cached_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Option<Vec<pipedash_plugin_api::WorkflowParameter>>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        let result = conn.query_row(
            "SELECT parameters_json FROM workflow_parameters_cache WHERE workflow_id = ?1",
            params![workflow_id],
            |row| {
                let json: String = row.get(0)?;
                Ok(json)
            },
        );

        match result {
            Ok(json) => {
                let parameters: Vec<pipedash_plugin_api::WorkflowParameter> =
                    serde_json::from_str(&json)
                        .map_err(|e| DomainError::DatabaseError(e.to_string()))?;
                Ok(Some(parameters))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DomainError::DatabaseError(e.to_string())),
        }
    }

    pub fn clear_workflow_parameters_cache(&self) -> DomainResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        conn.execute("DELETE FROM workflow_parameters_cache", [])
            .map_err(|e| DomainError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}
