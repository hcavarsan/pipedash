use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use sha2::{
    Digest,
    Sha256,
};
use sqlx::sqlite::{
    SqliteConnectOptions,
    SqliteJournalMode,
    SqlitePoolOptions,
};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Production,
    Sandbox,
}

impl Environment {
    fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Sandbox => "sandbox",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "production" => Some(Self::Production),
            "sandbox" => Some(Self::Sandbox),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub device_token: String,
    pub environment: Environment,
    pub bundle_id: String,
}

pub fn subscription_key(subscription_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(subscription_id.as_bytes());

    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn insert(
        &self, key: &str, subscription: &Subscription, now: u64, max_per_token: u32,
    ) -> sqlx::Result<()> {
        let now = i64::try_from(now).unwrap_or(i64::MAX);
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO subscriptions
                 (id_hash, device_token, environment, bundle_id, created_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        )
        .bind(key)
        .bind(&subscription.device_token)
        .bind(subscription.environment.as_str())
        .bind(&subscription.bundle_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM subscriptions
             WHERE device_token = ?1
               AND rowid NOT IN (
                   SELECT rowid FROM subscriptions
                   WHERE device_token = ?1
                   ORDER BY rowid DESC
                   LIMIT ?2
               )",
        )
        .bind(&subscription.device_token)
        .bind(i64::from(max_per_token))
        .execute(&mut *tx)
        .await?;

        tx.commit().await
    }

    pub async fn get(&self, key: &str) -> sqlx::Result<Option<Subscription>> {
        let row = sqlx::query_as::<_, (String, String, String)>(
            "SELECT device_token, environment, bundle_id FROM subscriptions WHERE id_hash = ?1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|(device_token, environment, bundle_id)| {
            let environment = Environment::parse(&environment)
                .ok_or_else(|| sqlx::Error::Decode("unknown environment".into()))?;

            Ok(Subscription {
                device_token,
                environment,
                bundle_id,
            })
        })
        .transpose()
    }

    pub async fn update_token(
        &self, key: &str, device_token: &str, now: u64,
    ) -> sqlx::Result<bool> {
        let result = sqlx::query(
            "UPDATE subscriptions SET device_token = ?1, last_active_at = ?2 WHERE id_hash = ?3",
        )
        .bind(device_token)
        .bind(i64::try_from(now).unwrap_or(i64::MAX))
        .bind(key)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn touch(&self, key: &str, now: u64) -> sqlx::Result<()> {
        sqlx::query("UPDATE subscriptions SET last_active_at = ?1 WHERE id_hash = ?2")
            .bind(i64::try_from(now).unwrap_or(i64::MAX))
            .bind(key)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_inactive_since(&self, cutoff: u64) -> sqlx::Result<u64> {
        let result = sqlx::query("DELETE FROM subscriptions WHERE last_active_at < ?1")
            .bind(i64::try_from(cutoff).unwrap_or(i64::MAX))
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete_if_token(&self, key: &str, device_token: &str) -> sqlx::Result<bool> {
        let result =
            sqlx::query("DELETE FROM subscriptions WHERE id_hash = ?1 AND device_token = ?2")
                .bind(key)
                .bind(device_token)
                .execute(&self.pool)
                .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, key: &str) -> sqlx::Result<bool> {
        let result = sqlx::query("DELETE FROM subscriptions WHERE id_hash = ?1")
            .bind(key)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests;
