use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::{
    Row as SqlxRow,
    SqlitePool,
};
use tokio::time::sleep;

use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::config_backend::{
    ConfigBackend,
    ConfigExport,
    StoredPermissions,
};

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

pub struct SqliteConfigBackend {
    pool: SqlitePool,
}

impl SqliteConfigBackend {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl ConfigBackend for SqliteConfigBackend {
    async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        retry_on_busy(|| async {
            let rows = sqlx::query(
                r#"SELECT id, name, provider_type, token_encrypted, config_json, refresh_interval, version FROM providers"#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to list providers: {}", e)))?;

            let mut providers = Vec::new();
            for row in rows {
                let id: i64 = row.get("id");
                let config_json: String = row.get("config_json");
                let config: HashMap<String, String> = if config_json.trim().is_empty() {
                    HashMap::new()
                } else {
                    serde_json::from_str(&config_json).map_err(|e| {
                        DomainError::DatabaseError(format!(
                            "Failed to parse config for {}: {}",
                            id, e
                        ))
                    })?
                };

                let token_ref: String = row.get("token_encrypted");

                providers.push(ProviderConfig {
                    id: Some(id),
                    name: row.get("name"),
                    provider_type: row.get("provider_type"),
                    config,
                    token: token_ref,
                    refresh_interval: row.get("refresh_interval"),
                    version: Some(row.get("version")),
                });
            }

            Ok(providers)
        })
        .await
    }

    async fn get_provider(&self, id: i64) -> DomainResult<Option<ProviderConfig>> {
        retry_on_busy(|| async {
            let row = sqlx::query(
                r#"SELECT id, name, provider_type, token_encrypted, config_json, refresh_interval, version FROM providers WHERE id = ?"#,
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to get provider: {}", e)))?;

            match row {
                Some(row) => {
                    let config_json: String = row.get("config_json");
                    let config: HashMap<String, String> = if config_json.trim().is_empty() {
                        HashMap::new()
                    } else {
                        serde_json::from_str(&config_json).map_err(|e| {
                            DomainError::DatabaseError(format!("Failed to parse config: {}", e))
                        })?
                    };

                    let token_ref: String = row.get("token_encrypted");

                    Ok(Some(ProviderConfig {
                        id: Some(id),
                        name: row.get("name"),
                        provider_type: row.get("provider_type"),
                        config,
                        token: token_ref,
                        refresh_interval: row.get("refresh_interval"),
                        version: Some(row.get("version")),
                    }))
                }
                None => Ok(None),
            }
        })
        .await
    }

    async fn create_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        retry_on_busy(|| async {
            let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM providers WHERE name = ?")
                .bind(&config.name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to check existing: {}", e))
                })?;

            if existing.is_some() {
                return Err(DomainError::InvalidConfig(format!(
                    "Provider '{}' already exists",
                    config.name
                )));
            }

            let config_json = serde_json::to_string(&config.config)
                .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize config: {}", e)))?;

            let result = sqlx::query(
                r#"INSERT INTO providers (name, provider_type, token_encrypted, config_json, refresh_interval)
                   VALUES (?, ?, ?, ?, ?)"#,
            )
            .bind(&config.name)
            .bind(&config.provider_type)
            .bind(&config.token)
            .bind(&config_json)
            .bind(config.refresh_interval)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to insert provider: {}", e)))?;

            Ok(result.last_insert_rowid())
        })
        .await
    }

    async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        retry_on_busy(|| async {
            let config_json = serde_json::to_string(&config.config)
                .map_err(|e| DomainError::DatabaseError(format!("Failed to serialize config: {}", e)))?;

            let result = sqlx::query(
                r#"UPDATE providers
                   SET name = ?, provider_type = ?, token_encrypted = ?, config_json = ?, refresh_interval = ?, updated_at = datetime('now')
                   WHERE id = ?"#,
            )
            .bind(&config.name)
            .bind(&config.provider_type)
            .bind(&config.token)
            .bind(&config_json)
            .bind(config.refresh_interval)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to update provider: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(DomainError::ProviderNotFound(format!("Provider {} not found", id)));
            }

            Ok(())
        })
        .await
    }

    async fn delete_provider(&self, id: i64) -> DomainResult<()> {
        retry_on_busy(|| async {
            let _ = sqlx::query("DELETE FROM pipelines_cache WHERE provider_id = ?")
                .bind(id)
                .execute(&self.pool)
                .await;

            let _ = sqlx::query("DELETE FROM provider_permissions WHERE provider_id = ?")
                .bind(id)
                .execute(&self.pool)
                .await;

            let _ = sqlx::query("DELETE FROM table_preferences WHERE provider_id = ?")
                .bind(id)
                .execute(&self.pool)
                .await;

            let result = sqlx::query("DELETE FROM providers WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to delete provider: {}", e))
                })?;

            if result.rows_affected() == 0 {
                return Err(DomainError::ProviderNotFound(format!(
                    "Provider {} not found",
                    id
                )));
            }

            Ok(())
        })
        .await
    }

    async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        retry_on_busy(|| async {
            let result = sqlx::query_scalar::<_, String>(
                "SELECT preferences_json FROM table_preferences WHERE provider_id = ? AND table_id = ?",
            )
            .bind(provider_id)
            .bind(table_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to get preferences: {}", e)))?;

            Ok(result)
        })
        .await
    }

    async fn set_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()> {
        retry_on_busy(|| async {
            sqlx::query(
                r#"INSERT INTO table_preferences (provider_id, table_id, preferences_json, created_at, updated_at)
                   VALUES (?, ?, ?, datetime('now'), datetime('now'))
                   ON CONFLICT(provider_id, table_id) DO UPDATE SET
                       preferences_json = excluded.preferences_json,
                       updated_at = datetime('now')"#,
            )
            .bind(provider_id)
            .bind(table_id)
            .bind(preferences_json)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to save preferences: {}", e)))?;

            Ok(())
        })
        .await
    }

    async fn store_permissions(
        &self, provider_id: i64, permissions: &StoredPermissions,
    ) -> DomainResult<()> {
        let start = std::time::Instant::now();
        let permissions_count = permissions.permissions.len();

        retry_on_busy(|| async {
            let checked_at_str = permissions.last_checked.to_rfc3339();

            sqlx::query("DELETE FROM provider_permissions WHERE provider_id = ?")
                .bind(provider_id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to clear permissions: {}", e))
                })?;

            if permissions.permissions.is_empty() {
                return Ok(());
            }

            const BATCH_SIZE: usize = 100;

            let perms_vec: Vec<_> = permissions.permissions.iter().collect();

            for chunk in perms_vec.chunks(BATCH_SIZE) {
                let values_clause = chunk
                    .iter()
                    .map(|_| "(?, ?, ?, ?)")
                    .collect::<Vec<_>>()
                    .join(", ");

                let sql = format!(
                    r#"INSERT INTO provider_permissions (provider_id, permission_name, granted, checked_at)
                       VALUES {}"#,
                    values_clause
                );

                let mut query = sqlx::query(&sql);
                for (permission_name, granted) in chunk {
                    query = query
                        .bind(provider_id)
                        .bind(permission_name)
                        .bind(**granted)
                        .bind(&checked_at_str);
                }

                query
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        DomainError::DatabaseError(format!("Failed to store permission: {}", e))
                    })?;
            }

            Ok(())
        })
        .await?;

        let elapsed = start.elapsed();
        tracing::debug!(
            provider_id = provider_id,
            permissions_count = permissions_count,
            elapsed_ms = elapsed.as_millis(),
            "Stored provider permissions (batch insert)"
        );

        Ok(())
    }

    async fn get_permissions(&self, provider_id: i64) -> DomainResult<Option<StoredPermissions>> {
        retry_on_busy(|| async {
            let rows = sqlx::query(
                r#"SELECT permission_name, granted, checked_at
                   FROM provider_permissions
                   WHERE provider_id = ?"#,
            )
            .bind(provider_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to get permissions: {}", e)))?;

            if rows.is_empty() {
                return Ok(None);
            }

            let checked_at_str: String = rows[0].get("checked_at");
            let last_checked = chrono::DateTime::parse_from_rfc3339(&checked_at_str)
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to parse timestamp: {}", e))
                })?
                .with_timezone(&chrono::Utc);

            let mut permissions = HashMap::new();
            for row in rows {
                let permission_name: String = row.get("permission_name");
                let granted: bool = row.get("granted");
                permissions.insert(permission_name, granted);
            }

            Ok(Some(StoredPermissions {
                permissions,
                last_checked,
            }))
        })
        .await
    }

    async fn export_all(&self) -> DomainResult<ConfigExport> {
        let providers = self.list_providers().await?;

        let pref_rows =
            sqlx::query(r#"SELECT provider_id, table_id, preferences_json FROM table_preferences"#)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to export preferences: {}", e))
                })?;

        let mut table_preferences = HashMap::new();
        for row in pref_rows {
            let provider_id: i64 = row.get("provider_id");
            let table_id: String = row.get("table_id");
            let preferences_json: String = row.get("preferences_json");
            let key = format!("{}:{}", provider_id, table_id);
            table_preferences.insert(key, preferences_json);
        }

        let perm_rows = sqlx::query(
            r#"SELECT provider_id, permission_name, granted, checked_at FROM provider_permissions"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to export permissions: {}", e)))?;

        let mut permissions: HashMap<i64, StoredPermissions> = HashMap::new();
        for row in perm_rows {
            let provider_id: i64 = row.get("provider_id");
            let permission_name: String = row.get("permission_name");
            let granted: bool = row.get("granted");
            let checked_at_str: String = row.get("checked_at");

            let last_checked = chrono::DateTime::parse_from_rfc3339(&checked_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let entry = permissions
                .entry(provider_id)
                .or_insert_with(|| StoredPermissions {
                    permissions: HashMap::new(),
                    last_checked,
                });
            entry.permissions.insert(permission_name, granted);
        }

        Ok(ConfigExport {
            version: "1.0".to_string(),
            providers,
            table_preferences,
            permissions,
        })
    }

    async fn import_all(&self, data: &ConfigExport) -> DomainResult<HashMap<i64, i64>> {
        let mut id_mapping: HashMap<i64, i64> = HashMap::new();

        for provider in &data.providers {
            let old_id = provider.id.unwrap_or(0);

            let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM providers WHERE name = ?")
                .bind(&provider.name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to check existing: {}", e))
                })?;

            let new_id = if let Some(existing_id) = existing {
                existing_id
            } else {
                self.create_provider(provider).await?
            };

            if old_id != 0 {
                id_mapping.insert(old_id, new_id);
            }
        }

        for (key, preferences_json) in &data.table_preferences {
            let parts: Vec<&str> = key.splitn(2, ':').collect();
            if parts.len() == 2 {
                if let Ok(old_provider_id) = parts[0].parse::<i64>() {
                    let table_id = parts[1];
                    let new_provider_id = id_mapping
                        .get(&old_provider_id)
                        .copied()
                        .unwrap_or(old_provider_id);
                    if self.get_provider(new_provider_id).await?.is_some() {
                        self.set_table_preferences(new_provider_id, table_id, preferences_json)
                            .await?;
                    }
                }
            }
        }

        for (old_provider_id, perms) in &data.permissions {
            let new_provider_id = id_mapping
                .get(old_provider_id)
                .copied()
                .unwrap_or(*old_provider_id);
            if self.get_provider(new_provider_id).await?.is_some() {
                self.store_permissions(new_provider_id, perms).await?;
            }
        }

        Ok(id_mapping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::init_database;

    #[tokio::test]
    async fn test_sqlite_backend_providers() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = init_database(db_path).await.unwrap();
        let backend = SqliteConfigBackend::new(pool);

        let config = ProviderConfig {
            id: None,
            name: "Test Provider".to_string(),
            provider_type: "github".to_string(),
            config: HashMap::from([("owner".to_string(), "test".to_string())]),
            token: String::new(),
            refresh_interval: 30,
            version: None,
        };

        let id = backend.create_provider(&config).await.unwrap();
        assert!(id > 0);

        let retrieved = backend.get_provider(id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Test Provider");
        assert_eq!(retrieved.provider_type, "github");

        let all = backend.list_providers().await.unwrap();
        assert_eq!(all.len(), 1);

        let updated_config = ProviderConfig {
            id: Some(id),
            name: "Updated Provider".to_string(),
            provider_type: "github".to_string(),
            config: HashMap::from([("owner".to_string(), "updated".to_string())]),
            token: String::new(),
            refresh_interval: 60,
            version: None,
        };
        backend.update_provider(id, &updated_config).await.unwrap();

        let retrieved = backend.get_provider(id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Provider");
        assert_eq!(retrieved.refresh_interval, 60);

        backend.delete_provider(id).await.unwrap();
        assert!(backend.get_provider(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sqlite_backend_table_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = init_database(db_path).await.unwrap();
        let backend = SqliteConfigBackend::new(pool);

        let config = ProviderConfig {
            id: None,
            name: "Prefs Provider".to_string(),
            provider_type: "github".to_string(),
            config: HashMap::new(),
            token: String::new(),
            refresh_interval: 30,
            version: None,
        };
        let provider_id = backend.create_provider(&config).await.unwrap();

        let prefs_json = r#"{"columns":["name","status"],"sort":"name"}"#;
        backend
            .set_table_preferences(provider_id, "pipelines", prefs_json)
            .await
            .unwrap();

        let retrieved = backend
            .get_table_preferences(provider_id, "pipelines")
            .await
            .unwrap();
        assert_eq!(retrieved, Some(prefs_json.to_string()));

        let missing = backend
            .get_table_preferences(provider_id, "other")
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_sqlite_backend_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = init_database(db_path).await.unwrap();
        let backend = SqliteConfigBackend::new(pool);

        let config = ProviderConfig {
            id: None,
            name: "Perms Provider".to_string(),
            provider_type: "github".to_string(),
            config: HashMap::new(),
            token: String::new(),
            refresh_interval: 30,
            version: None,
        };
        let provider_id = backend.create_provider(&config).await.unwrap();

        let mut perms_map = HashMap::new();
        perms_map.insert("repo:read".to_string(), true);
        perms_map.insert("repo:write".to_string(), false);
        let permissions = StoredPermissions {
            permissions: perms_map,
            last_checked: chrono::Utc::now(),
        };
        backend
            .store_permissions(provider_id, &permissions)
            .await
            .unwrap();

        let retrieved = backend.get_permissions(provider_id).await.unwrap().unwrap();
        assert_eq!(retrieved.permissions.get("repo:read"), Some(&true));
        assert_eq!(retrieved.permissions.get("repo:write"), Some(&false));
    }

    #[tokio::test]
    async fn test_sqlite_backend_export_import() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = init_database(db_path).await.unwrap();
        let backend = SqliteConfigBackend::new(pool);

        let config = ProviderConfig {
            id: None,
            name: "Export Test".to_string(),
            provider_type: "gitlab".to_string(),
            config: HashMap::from([("host".to_string(), "gitlab.com".to_string())]),
            token: String::new(),
            refresh_interval: 45,
            version: None,
        };
        backend.create_provider(&config).await.unwrap();

        let export = backend.export_all().await.unwrap();
        assert_eq!(export.providers.len(), 1);
        assert_eq!(export.providers[0].name, "Export Test");

        let dir2 = tempfile::tempdir().unwrap();
        let db_path2 = dir2.path().join("test2.db");
        let pool2 = init_database(db_path2).await.unwrap();
        let backend2 = SqliteConfigBackend::new(pool2);

        backend2.import_all(&export).await.unwrap();

        let imported = backend2.list_providers().await.unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "Export Test");
    }
}
