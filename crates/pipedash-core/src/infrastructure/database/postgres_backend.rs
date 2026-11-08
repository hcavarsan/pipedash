use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::postgres::{
    PgPool,
    PgRow,
};
use sqlx::Row;

use super::{
    config_transfer,
    provider_row::{
        ensure_found,
        ensure_unique,
        failed,
        provider_config,
        serialize_config,
    },
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

const PROVIDER_SELECT: &str =
    "SELECT id, name, provider_type, config_json, refresh_interval, version FROM providers";

fn provider_from_row(row: &PgRow) -> DomainResult<ProviderConfig> {
    provider_config(
        row.get("id"),
        row.get("name"),
        row.get("provider_type"),
        String::new(),
        &row.get::<String, _>("config_json"),
        row.get::<i32, _>("refresh_interval") as i64,
        row.get::<i32, _>("version") as i64,
    )
}

pub struct PostgresConfigBackend {
    pool: PgPool,
}

impl PostgresConfigBackend {
    pub async fn new(connection_string: &str) -> DomainResult<Self> {
        let pool = crate::infrastructure::database::init_postgres_database(connection_string)
            .await
            .map_err(failed("initialize PostgreSQL"))?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl ConfigBackend for PostgresConfigBackend {
    async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        let rows = sqlx::query(PROVIDER_SELECT)
            .fetch_all(&self.pool)
            .await
            .map_err(failed("list providers"))?;

        rows.iter().map(provider_from_row).collect()
    }

    async fn get_provider(&self, id: i64) -> DomainResult<Option<ProviderConfig>> {
        let sql = format!("{PROVIDER_SELECT} WHERE id = $1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(failed("get provider"))?;

        row.map(|row| provider_from_row(&row)).transpose()
    }

    async fn create_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM providers WHERE name = $1")
            .bind(&config.name)
            .fetch_optional(&self.pool)
            .await
            .map_err(failed("check existing"))?;
        ensure_unique(existing, &config.name)?;

        let config_json = serialize_config(config)?;

        sqlx::query_scalar(
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
        .map_err(failed("insert provider"))
    }

    async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        let config_json = serialize_config(config)?;

        let affected = sqlx::query(
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
        .map(|result| result.rows_affected())
        .map_err(failed("update provider"))?;

        ensure_found(affected, id)
    }

    async fn delete_provider(&self, id: i64) -> DomainResult<()> {
        let affected = sqlx::query("DELETE FROM providers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected())
            .map_err(failed("delete provider"))?;

        ensure_found(affected, id)
    }

    async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        sqlx::query_scalar(
            "SELECT preferences_json FROM table_preferences WHERE provider_id = $1 AND table_id = $2",
        )
        .bind(provider_id)
        .bind(table_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(failed("get preferences"))
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
        .map_err(failed("save preferences"))?;

        Ok(())
    }

    async fn store_permissions(
        &self, provider_id: i64, permissions: &StoredPermissions,
    ) -> DomainResult<()> {
        sqlx::query("DELETE FROM provider_permissions WHERE provider_id = $1")
            .bind(provider_id)
            .execute(&self.pool)
            .await
            .map_err(failed("clear permissions"))?;

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
            .map_err(failed("store permission"))?;
        }

        Ok(())
    }

    async fn export_all(&self) -> DomainResult<ConfigExport> {
        config_transfer::export_config(self, &DatabasePool::Postgres(self.pool.clone())).await
    }

    async fn import_all(&self, data: &ConfigExport) -> DomainResult<HashMap<i64, i64>> {
        config_transfer::import_config(self, &DatabasePool::Postgres(self.pool.clone()), data).await
    }
}
