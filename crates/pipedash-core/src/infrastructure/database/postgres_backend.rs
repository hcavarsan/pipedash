use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::postgres::PgPool;
use sqlx::Row;

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

pub struct PostgresConfigBackend {
    pool: PgPool,
}

impl PostgresConfigBackend {
    pub async fn new(connection_string: &str) -> DomainResult<Self> {
        let pool = crate::infrastructure::database::init_postgres_database(connection_string)
            .await
            .map_err(|e| {
                DomainError::DatabaseError(format!("Failed to initialize PostgreSQL: {}", e))
            })?;

        Ok(Self { pool })
    }

    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl ConfigBackend for PostgresConfigBackend {
    async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        let rows = sqlx::query(
            "SELECT id, name, provider_type, config_json, refresh_interval, version FROM providers",
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
                    DomainError::DatabaseError(format!("Failed to parse config for {}: {}", id, e))
                })?
            };

            providers.push(ProviderConfig {
                id: Some(id),
                name: row.get("name"),
                provider_type: row.get("provider_type"),
                config,
                token: String::new(), // Tokens stored separately in TokenStore
                refresh_interval: row.get::<i32, _>("refresh_interval") as i64,
                version: Some(row.get::<i32, _>("version") as i64),
            });
        }

        Ok(providers)
    }

    async fn get_provider(&self, id: i64) -> DomainResult<Option<ProviderConfig>> {
        let row = sqlx::query(
            "SELECT id, name, provider_type, config_json, refresh_interval, version FROM providers WHERE id = $1",
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

                Ok(Some(ProviderConfig {
                    id: Some(id),
                    name: row.get("name"),
                    provider_type: row.get("provider_type"),
                    config,
                    token: String::new(),
                    refresh_interval: row.get::<i32, _>("refresh_interval") as i64,
                    version: Some(row.get::<i32, _>("version") as i64),
                }))
            }
            None => Ok(None),
        }
    }

    async fn create_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM providers WHERE name = $1")
            .bind(&config.name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to check existing: {}", e)))?;

        if existing.is_some() {
            return Err(DomainError::InvalidConfig(format!(
                "Provider '{}' already exists",
                config.name
            )));
        }

        let config_json = serde_json::to_string(&config.config).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to serialize config: {}", e))
        })?;

        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO providers (name, provider_type, token_encrypted, config_json, refresh_interval)
            VALUES ($1, $2, '', $3, $4)
            RETURNING id
            "#,
        )
        .bind(&config.name)
        .bind(&config.provider_type)
        .bind(&config_json)
        .bind(config.refresh_interval)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to insert provider: {}", e)))?;

        Ok(id)
    }

    async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        let config_json = serde_json::to_string(&config.config).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to serialize config: {}", e))
        })?;

        let result = sqlx::query(
            r#"
            UPDATE providers
            SET name = $1, provider_type = $2, config_json = $3, refresh_interval = $4, updated_at = NOW()
            WHERE id = $5
            "#,
        )
        .bind(&config.name)
        .bind(&config.provider_type)
        .bind(&config_json)
        .bind(config.refresh_interval)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to update provider: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::ProviderNotFound(format!(
                "Provider {} not found",
                id
            )));
        }

        Ok(())
    }

    async fn delete_provider(&self, id: i64) -> DomainResult<()> {
        let result = sqlx::query("DELETE FROM providers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to delete provider: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::ProviderNotFound(format!(
                "Provider {} not found",
                id
            )));
        }

        Ok(())
    }

    async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT preferences_json FROM table_preferences WHERE provider_id = $1 AND table_id = $2",
        )
        .bind(provider_id)
        .bind(table_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to get preferences: {}", e)))?;

        Ok(result)
    }

    async fn set_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()> {
        sqlx::query(
            r#"
            INSERT INTO table_preferences (provider_id, table_id, preferences_json)
            VALUES ($1, $2, $3)
            ON CONFLICT (provider_id, table_id) DO UPDATE SET
                preferences_json = EXCLUDED.preferences_json,
                updated_at = NOW()
            "#,
        )
        .bind(provider_id)
        .bind(table_id)
        .bind(preferences_json)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to save preferences: {}", e)))?;

        Ok(())
    }

    async fn store_permissions(
        &self, provider_id: i64, permissions: &StoredPermissions,
    ) -> DomainResult<()> {
        sqlx::query("DELETE FROM provider_permissions WHERE provider_id = $1")
            .bind(provider_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::DatabaseError(format!("Failed to clear permissions: {}", e))
            })?;

        for (permission_name, granted) in &permissions.permissions {
            sqlx::query(
                r#"
                INSERT INTO provider_permissions (provider_id, permission_name, granted, checked_at)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(provider_id)
            .bind(permission_name)
            .bind(*granted)
            .bind(permissions.last_checked)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::DatabaseError(format!("Failed to store permission: {}", e))
            })?;
        }

        Ok(())
    }

    async fn get_permissions(&self, provider_id: i64) -> DomainResult<Option<StoredPermissions>> {
        let rows = sqlx::query(
            "SELECT permission_name, granted, checked_at FROM provider_permissions WHERE provider_id = $1",
        )
        .bind(provider_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to get permissions: {}", e)))?;

        if rows.is_empty() {
            return Ok(None);
        }

        let last_checked: chrono::DateTime<chrono::Utc> = rows[0].get("checked_at");

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
    }

    async fn export_all(&self) -> DomainResult<ConfigExport> {
        let providers = self.list_providers().await?;

        let pref_rows =
            sqlx::query("SELECT provider_id, table_id, preferences_json FROM table_preferences")
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
            "SELECT provider_id, permission_name, granted, checked_at FROM provider_permissions",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to export permissions: {}", e)))?;

        let mut permissions: HashMap<i64, StoredPermissions> = HashMap::new();
        for row in perm_rows {
            let provider_id: i64 = row.get("provider_id");
            let permission_name: String = row.get("permission_name");
            let granted: bool = row.get("granted");
            let last_checked: chrono::DateTime<chrono::Utc> = row.get("checked_at");

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

            let existing: Option<i64> =
                sqlx::query_scalar("SELECT id FROM providers WHERE name = $1")
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
