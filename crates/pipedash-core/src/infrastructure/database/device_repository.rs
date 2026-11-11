use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};

use super::bind::{
    delete_by_key,
    execute_bound,
    fetch_all_as,
    fetch_optional_as,
    set_timestamp,
    BindValue,
};
use super::pool::db_err;
use super::DatabasePool;
use crate::domain::{
    DomainError,
    DomainResult,
    NotificationPreferences,
    PairedDevice,
};

type Row = (
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

const COLUMNS: &str = "id, token_hash, name, platform, created_at, last_seen_at";
const UPDATE_ATTEMPTS: usize = 8;

fn device(row: Row) -> PairedDevice {
    let (id, token_hash, name, platform, created_at, last_seen_at) = row;

    PairedDevice {
        id,
        token_hash,
        name,
        platform,
        created_at,
        last_seen_at,
    }
}

pub struct DeviceRepository {
    pool: DatabasePool,
}

impl DeviceRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn insert_sql(&self, suffix: &str) -> String {
        format!(
            "INSERT INTO paired_devices ({COLUMNS}) VALUES ({}){suffix}",
            self.pool.placeholders(6)
        )
    }

    pub async fn insert(&self, device: &PairedDevice) -> DomainResult<()> {
        self.execute_insert(self.insert_sql(""), device).await
    }

    pub async fn list(&self) -> DomainResult<Vec<PairedDevice>> {
        let sql = format!("SELECT {COLUMNS} FROM paired_devices ORDER BY created_at, id");
        let rows: Vec<Row> = fetch_all_as(&self.pool, &sql, &[]).await?;

        Ok(rows.into_iter().map(device).collect())
    }

    pub async fn find_by_token(&self, token: &str) -> DomainResult<Option<PairedDevice>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM paired_devices WHERE token_hash = {}",
            self.pool.placeholder(1)
        );
        let token_hash = PairedDevice::hash_token(token);
        let values = [BindValue::Text(&token_hash)];
        let row: Option<Row> = fetch_optional_as(&self.pool, &sql, &values).await?;

        Ok(row.map(device))
    }

    pub async fn touch(&self, id: &str) -> DomainResult<()> {
        set_timestamp(
            &self.pool,
            "paired_devices",
            "last_seen_at",
            "id",
            id,
            Utc::now(),
        )
        .await
    }

    pub async fn set_notification_preferences(
        &self, id: &str, preferences: &NotificationPreferences,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE paired_devices SET notification_preferences = {} WHERE id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let json = serde_json::to_string(preferences).map_err(db_err)?;
        let values = [BindValue::Text(&json), BindValue::Text(id)];
        let updated = execute_bound(&self.pool, &sql, &values).await?;

        Ok(updated > 0)
    }

    pub async fn update_notification_preferences(
        &self, id: &str, mut change: impl FnMut(&mut NotificationPreferences) -> bool,
    ) -> DomainResult<Option<NotificationPreferences>> {
        for _ in 0..UPDATE_ATTEMPTS {
            let Some(stored) = self.preferences_json(id).await? else {
                return Ok(None);
            };
            let mut preferences: NotificationPreferences = stored
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_default();
            if !change(&mut preferences) {
                return Ok(Some(preferences));
            }
            let json = serde_json::to_string(&preferences).map_err(db_err)?;
            if self.swap_preferences(id, stored.as_deref(), &json).await? {
                return Ok(Some(preferences));
            }
        }

        Err(DomainError::DatabaseError(
            "Notification preferences kept changing while being saved".into(),
        ))
    }

    async fn preferences_json(&self, id: &str) -> DomainResult<Option<Option<String>>> {
        let sql = format!(
            "SELECT notification_preferences FROM paired_devices WHERE id = {}",
            self.pool.placeholder(1)
        );
        let row: Option<(Option<String>,)> =
            fetch_optional_as(&self.pool, &sql, &[BindValue::Text(id)]).await?;

        Ok(row.map(|(json,)| json))
    }

    async fn swap_preferences(
        &self, id: &str, expected: Option<&str>, json: &str,
    ) -> DomainResult<bool> {
        let same = match &self.pool {
            DatabasePool::Sqlite(_) => "IS",
            DatabasePool::Postgres(_) => "IS NOT DISTINCT FROM",
        };
        let sql = format!(
            "UPDATE paired_devices SET notification_preferences = {} \
             WHERE id = {} AND notification_preferences {same} {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3)
        );
        let values = [
            BindValue::Text(json),
            BindValue::Text(id),
            BindValue::OptText(expected),
        ];
        let updated = execute_bound(&self.pool, &sql, &values).await?;

        Ok(updated > 0)
    }

    pub async fn notification_preferences(
        &self,
    ) -> DomainResult<HashMap<String, NotificationPreferences>> {
        let sql = "SELECT id, notification_preferences FROM paired_devices \
                   WHERE notification_preferences IS NOT NULL";
        let rows: Vec<(String, String)> = fetch_all_as(&self.pool, sql, &[]).await?;

        Ok(rows
            .into_iter()
            .filter_map(|(id, json)| Some((id, serde_json::from_str(&json).ok()?)))
            .collect())
    }

    pub async fn delete(&self, id: &str) -> DomainResult<bool> {
        delete_by_key(&self.pool, "paired_devices", "id", id).await
    }

    pub async fn delete_all(&self) -> DomainResult<u64> {
        execute_bound(&self.pool, "DELETE FROM paired_devices", &[]).await
    }

    pub async fn import(&self, devices: &[PairedDevice]) -> DomainResult<()> {
        let sql = self.insert_sql(" ON CONFLICT (id) DO NOTHING");
        for device in devices {
            self.execute_insert(sql.clone(), device).await?;
        }

        Ok(())
    }

    async fn execute_insert(&self, sql: String, device: &PairedDevice) -> DomainResult<()> {
        let values = [
            BindValue::Text(&device.id),
            BindValue::Text(&device.token_hash),
            BindValue::Text(&device.name),
            BindValue::Text(&device.platform),
            BindValue::Timestamp(device.created_at),
            BindValue::OptTimestamp(device.last_seen_at),
        ];
        execute_bound(&self.pool, &sql, &values).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
