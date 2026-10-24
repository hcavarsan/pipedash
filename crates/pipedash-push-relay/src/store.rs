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

// The subscription id is a bearer secret, so only its hash is stored.
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

    // Keeps the newest `max_per_token` subscriptions of a device token, so
    // repeated registrations of one token cannot pile up rows.
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

    pub async fn delete(&self, key: &str) -> sqlx::Result<bool> {
        let result = sqlx::query("DELETE FROM subscriptions WHERE id_hash = ?1")
            .bind(key)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(token: &str) -> Subscription {
        Subscription {
            device_token: token.to_string(),
            environment: Environment::Sandbox,
            bundle_id: "com.example.app".to_string(),
        }
    }

    #[tokio::test]
    async fn a_subscription_round_trips_and_can_rotate_and_be_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
        let key = subscription_key("secret-id");

        assert_eq!(store.get(&key).await.unwrap(), None);
        store
            .insert(&key, &subscription("aa"), 1, 10)
            .await
            .unwrap();
        assert_eq!(store.get(&key).await.unwrap(), Some(subscription("aa")));

        assert!(store.update_token(&key, "bb", 2).await.unwrap());
        assert_eq!(store.get(&key).await.unwrap(), Some(subscription("bb")));
        assert!(!store.update_token("unknown", "cc", 2).await.unwrap());

        assert!(store.delete(&key).await.unwrap());
        assert!(!store.delete(&key).await.unwrap());
        assert_eq!(store.get(&key).await.unwrap(), None);
    }

    #[tokio::test]
    async fn the_subscription_id_itself_is_never_stored() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
        let key = subscription_key("secret-id");
        store
            .insert(&key, &subscription("aa"), 1, 10)
            .await
            .unwrap();

        let stored: Vec<(String,)> = sqlx::query_as("SELECT id_hash FROM subscriptions")
            .fetch_all(&store.pool)
            .await
            .unwrap();

        assert_eq!(stored, vec![(key.clone(),)]);
        assert_eq!(key.len(), 64);
        assert!(!key.contains("secret-id"));
    }

    #[tokio::test]
    async fn only_the_newest_subscriptions_of_one_device_token_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
        let other = subscription_key("other-token");
        store
            .insert(&other, &subscription("bb"), 1, 2)
            .await
            .unwrap();

        for i in 0..5 {
            let key = subscription_key(&format!("id-{i}"));
            store.insert(&key, &subscription("aa"), 1, 2).await.unwrap();
        }

        for (i, kept) in [(0, false), (1, false), (2, false), (3, true), (4, true)] {
            let key = subscription_key(&format!("id-{i}"));
            assert_eq!(store.get(&key).await.unwrap().is_some(), kept, "id-{i}");
        }
        assert!(store.get(&other).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn inactive_subscriptions_expire_but_rotation_and_pushes_keep_them_alive() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
        let [stale, rotated, pushed] = ["stale", "rotated", "pushed"].map(subscription_key);
        for key in [&stale, &rotated, &pushed] {
            store
                .insert(key, &subscription(key), 100, 10)
                .await
                .unwrap();
        }
        store.update_token(&rotated, "new", 500).await.unwrap();
        store.touch(&pushed, 500).await.unwrap();

        assert_eq!(store.delete_inactive_since(500).await.unwrap(), 1);

        assert!(store.get(&stale).await.unwrap().is_none());
        assert!(store.get(&rotated).await.unwrap().is_some());
        assert!(store.get(&pushed).await.unwrap().is_some());
        assert_eq!(store.delete_inactive_since(501).await.unwrap(), 2);
    }
}
