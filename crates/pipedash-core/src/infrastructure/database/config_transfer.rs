use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};

use super::DatabasePool;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::config_backend::{
    ConfigBackend,
    ConfigExport,
    StoredPermissions,
};

type PermissionRow = (i64, String, bool, DateTime<Utc>);

async fn permission_rows(pool: &DatabasePool) -> DomainResult<Vec<PermissionRow>> {
    let sql = "SELECT provider_id, permission_name, granted, checked_at FROM provider_permissions";
    let export_err =
        |e: sqlx::Error| DomainError::DatabaseError(format!("Failed to export permissions: {}", e));
    match pool {
        DatabasePool::Sqlite(p) => {
            let rows: Vec<(i64, String, bool, String)> =
                sqlx::query_as(sql).fetch_all(p).await.map_err(export_err)?;
            Ok(rows
                .into_iter()
                .map(|(id, name, granted, checked_at)| {
                    let at = DateTime::parse_from_rfc3339(&checked_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    (id, name, granted, at)
                })
                .collect())
        }
        DatabasePool::Postgres(p) => sqlx::query_as(sql).fetch_all(p).await.map_err(export_err),
    }
}

pub(super) async fn export_config(
    backend: &dyn ConfigBackend, pool: &DatabasePool,
) -> DomainResult<ConfigExport> {
    let providers = backend.list_providers().await?;

    let sql = "SELECT provider_id, table_id, preferences_json FROM table_preferences";
    let pref_rows: Vec<(i64, String, String)> = on_pool!(pool, p => sqlx::query_as(sql)
        .fetch_all(p)
        .await)
    .map_err(|e| DomainError::DatabaseError(format!("Failed to export preferences: {}", e)))?;
    let table_preferences = pref_rows
        .into_iter()
        .map(|(provider_id, table_id, json)| (format!("{}:{}", provider_id, table_id), json))
        .collect();

    let mut permissions: HashMap<i64, StoredPermissions> = HashMap::new();
    for (provider_id, name, granted, last_checked) in permission_rows(pool).await? {
        permissions
            .entry(provider_id)
            .or_insert_with(|| StoredPermissions {
                permissions: HashMap::new(),
                last_checked,
            })
            .permissions
            .insert(name, granted);
    }

    Ok(ConfigExport {
        version: "1.0".to_string(),
        providers,
        table_preferences,
        permissions,
    })
}

pub(super) async fn import_config(
    backend: &dyn ConfigBackend, pool: &DatabasePool, data: &ConfigExport,
) -> DomainResult<HashMap<i64, i64>> {
    let mut id_mapping: HashMap<i64, i64> = HashMap::new();
    let sql = format!(
        "SELECT id FROM providers WHERE name = {}",
        pool.placeholder(1)
    );

    for provider in &data.providers {
        let old_id = provider.id.unwrap_or(0);

        let existing: Option<i64> =
            on_pool!(pool, p => sqlx::query_scalar(sqlx::AssertSqlSafe(sql.clone()))
            .bind(&provider.name)
            .fetch_optional(p)
            .await)
            .map_err(|e| DomainError::DatabaseError(format!("Failed to check existing: {}", e)))?;

        let new_id = match existing {
            Some(existing_id) => existing_id,
            None => backend.create_provider(provider).await?,
        };

        if old_id != 0 {
            id_mapping.insert(old_id, new_id);
        }
    }

    for (key, preferences_json) in &data.table_preferences {
        let Some((provider, table_id)) = key.split_once(':') else {
            continue;
        };
        let Ok(old_provider_id) = provider.parse::<i64>() else {
            continue;
        };
        let new_provider_id = id_mapping
            .get(&old_provider_id)
            .copied()
            .unwrap_or(old_provider_id);
        if backend.get_provider(new_provider_id).await?.is_some() {
            backend
                .set_table_preferences(new_provider_id, table_id, preferences_json)
                .await?;
        }
    }

    for (old_provider_id, perms) in &data.permissions {
        let new_provider_id = id_mapping
            .get(old_provider_id)
            .copied()
            .unwrap_or(*old_provider_id);
        if backend.get_provider(new_provider_id).await?.is_some() {
            backend.store_permissions(new_provider_id, perms).await?;
        }
    }

    Ok(id_mapping)
}
