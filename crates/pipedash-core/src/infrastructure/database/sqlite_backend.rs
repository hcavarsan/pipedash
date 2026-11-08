use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;
use sqlx::{
    Row as SqlxRow,
    SqlitePool,
};

use super::provider_row::{
    ensure_found,
    ensure_unique,
    failed,
    provider_config,
    serialize_config,
};
use super::retry::retry_on_busy;
use super::{
    config_transfer,
    DatabasePool,
};
use crate::domain::{
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::config_backend::{
    ConfigBackend,
    ConfigExport,
    StoredPermissions,
};

const PROVIDER_SELECT: &str = "SELECT id, name, provider_type, token_encrypted, config_json, refresh_interval, version FROM providers";

fn provider_from_row(row: &SqliteRow) -> DomainResult<ProviderConfig> {
    provider_config(
        row.get("id"),
        row.get("name"),
        row.get("provider_type"),
        row.get("token_encrypted"),
        &row.get::<String, _>("config_json"),
        row.get("refresh_interval"),
        row.get("version"),
    )
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
            let rows = sqlx::query(PROVIDER_SELECT)
                .fetch_all(&self.pool)
                .await
                .map_err(failed("list providers"))?;

            rows.iter().map(provider_from_row).collect()
        })
        .await
    }

    async fn get_provider(&self, id: i64) -> DomainResult<Option<ProviderConfig>> {
        retry_on_busy(|| async {
            let sql = format!("{PROVIDER_SELECT} WHERE id = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(failed("get provider"))?;

            row.as_ref().map(provider_from_row).transpose()
        })
        .await
    }

    async fn create_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        retry_on_busy(|| async {
            let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM providers WHERE name = ?")
                .bind(&config.name)
                .fetch_optional(&self.pool)
                .await
                .map_err(failed("check existing"))?;
            ensure_unique(existing, &config.name)?;

            let config_json = serialize_config(config)?;

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
            .map_err(failed("insert provider"))?;

            Ok(result.last_insert_rowid())
        })
        .await
    }

    async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        retry_on_busy(|| async {
            let config_json = serialize_config(config)?;

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
            .map_err(failed("update provider"))?;

            ensure_found(result.rows_affected(), id)
        })
        .await
    }

    async fn delete_provider(&self, id: i64) -> DomainResult<()> {
        retry_on_busy(|| async {
            for table in [
                "DELETE FROM pipelines_cache WHERE provider_id = ?",
                "DELETE FROM provider_permissions WHERE provider_id = ?",
                "DELETE FROM table_preferences WHERE provider_id = ?",
            ] {
                let _ = sqlx::query(table).bind(id).execute(&self.pool).await;
            }

            let result = sqlx::query("DELETE FROM providers WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(failed("delete provider"))?;

            ensure_found(result.rows_affected(), id)
        })
        .await
    }

    async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        retry_on_busy(|| async {
            sqlx::query_scalar::<_, String>(
                "SELECT preferences_json FROM table_preferences WHERE provider_id = ? AND table_id = ?",
            )
            .bind(provider_id)
            .bind(table_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(failed("get preferences"))
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
            .map_err(failed("save preferences"))?;

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
                .map_err(failed("clear permissions"))?;

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

                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
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
                    .map_err(failed("store permission"))?;
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

    async fn export_all(&self) -> DomainResult<ConfigExport> {
        config_transfer::export_config(self, &DatabasePool::Sqlite(self.pool.clone())).await
    }

    async fn import_all(&self, data: &ConfigExport) -> DomainResult<HashMap<i64, i64>> {
        config_transfer::import_config(self, &DatabasePool::Sqlite(self.pool.clone()), data).await
    }
}

#[cfg(test)]
mod tests;
