use chrono::{
    DateTime,
    Utc,
};

use super::DatabasePool;
use crate::domain::{
    DomainError,
    DomainResult,
    PushSubscription,
};

type Row = (
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

const COLUMNS: &str =
    "id, relay_subscription_id, device_name, platform, created_at, last_success_at";

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseError(error.to_string())
}

fn subscription(row: Row) -> PushSubscription {
    let (id, relay_subscription_id, device_name, platform, created_at, last_success_at) = row;

    PushSubscription {
        id,
        relay_subscription_id,
        device_name,
        platform,
        created_at,
        last_success_at,
    }
}

pub struct PushRepository {
    pool: DatabasePool,
}

impl PushRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn placeholder(&self, index: usize) -> String {
        match &self.pool {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${index}"),
        }
    }

    pub async fn list(&self) -> DomainResult<Vec<PushSubscription>> {
        let sql = format!("SELECT {COLUMNS} FROM push_subscriptions ORDER BY created_at, id");
        let rows = match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
                    .fetch_all(p)
                    .await
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
                    .fetch_all(p)
                    .await
            }
        }
        .map_err(db_error)?;

        Ok(rows.into_iter().map(subscription).collect())
    }

    pub async fn get(&self, id: &str) -> DomainResult<Option<PushSubscription>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM push_subscriptions WHERE id = {}",
            self.placeholder(1)
        );
        let row = match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
                    .bind(id)
                    .fetch_optional(p)
                    .await
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
                    .bind(id)
                    .fetch_optional(p)
                    .await
            }
        }
        .map_err(db_error)?;

        Ok(row.map(subscription))
    }

    pub async fn register(
        &self, relay_subscription_id: &str, device_name: &str, platform: &str,
    ) -> DomainResult<PushSubscription> {
        let id = PushSubscription::local_id(relay_subscription_id);
        let sql = format!(
            "INSERT INTO push_subscriptions (id, relay_subscription_id, device_name, platform, created_at)
             VALUES ({}, {}, {}, {}, {})
             ON CONFLICT (id) DO NOTHING",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
        );
        let now = Utc::now();
        match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&id)
                .bind(relay_subscription_id)
                .bind(device_name)
                .bind(platform)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ()),
            DatabasePool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&id)
                .bind(relay_subscription_id)
                .bind(device_name)
                .bind(platform)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ()),
        }
        .map_err(db_error)?;

        self.get(&id).await?.ok_or_else(|| {
            DomainError::DatabaseError("Push subscription disappeared after insert".to_string())
        })
    }

    pub async fn delete(&self, id: &str) -> DomainResult<bool> {
        let sql = format!(
            "DELETE FROM push_subscriptions WHERE id = {}",
            self.placeholder(1)
        );
        let deleted = match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .execute(p)
                .await
                .map(|result| result.rows_affected()),
            DatabasePool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .execute(p)
                .await
                .map(|result| result.rows_affected()),
        }
        .map_err(db_error)?;

        Ok(deleted > 0)
    }

    pub async fn mark_success(&self, id: &str) -> DomainResult<()> {
        let sql = format!(
            "UPDATE push_subscriptions SET last_success_at = {} WHERE id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let now = Utc::now();
        match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(now)
                .bind(id)
                .execute(p)
                .await
                .map(|_| ()),
            DatabasePool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(now)
                .bind(id)
                .execute(p)
                .await
                .map(|_| ()),
        }
        .map_err(db_error)
    }

    pub async fn delete_all(&self) -> DomainResult<u64> {
        let sql = "DELETE FROM push_subscriptions";
        match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query(sql)
                .execute(p)
                .await
                .map(|result| result.rows_affected()),
            DatabasePool::Postgres(p) => sqlx::query(sql)
                .execute(p)
                .await
                .map(|result| result.rows_affected()),
        }
        .map_err(db_error)
    }

    pub async fn import(&self, subscriptions: &[PushSubscription]) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO push_subscriptions
                 (id, relay_subscription_id, device_name, platform, created_at, last_success_at)
             VALUES ({}, {}, {}, {}, {}, {})
             ON CONFLICT (id) DO UPDATE SET
                 relay_subscription_id = excluded.relay_subscription_id,
                 device_name = excluded.device_name,
                 platform = excluded.platform,
                 created_at = excluded.created_at,
                 last_success_at = excluded.last_success_at",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
        );

        for subscription in subscriptions {
            match &self.pool {
                DatabasePool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                    .bind(&subscription.id)
                    .bind(&subscription.relay_subscription_id)
                    .bind(&subscription.device_name)
                    .bind(&subscription.platform)
                    .bind(subscription.created_at)
                    .bind(subscription.last_success_at)
                    .execute(p)
                    .await
                    .map(|_| ()),
                DatabasePool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                    .bind(&subscription.id)
                    .bind(&subscription.relay_subscription_id)
                    .bind(&subscription.device_name)
                    .bind(&subscription.platform)
                    .bind(subscription.created_at)
                    .bind(subscription.last_success_at)
                    .execute(p)
                    .await
                    .map(|_| ()),
            }
            .map_err(db_error)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::init_database;

    async fn exercise(repository: &PushRepository) {
        repository
            .delete(&PushSubscription::local_id("relay-a"))
            .await
            .unwrap();
        repository
            .delete(&PushSubscription::local_id("relay-b"))
            .await
            .unwrap();

        let first = repository
            .register("relay-a", "Henrique's iPhone", "ios")
            .await
            .unwrap();
        assert_eq!(first.id, PushSubscription::local_id("relay-a"));
        assert_ne!(first.id, first.relay_subscription_id);
        assert_eq!(first.relay_subscription_id, "relay-a");
        assert_eq!(first.device_name, "Henrique's iPhone");
        assert_eq!(first.platform, "ios");
        assert_eq!(first.last_success_at, None);

        let again = repository
            .register("relay-a", "Renamed", "ios")
            .await
            .unwrap();
        assert_eq!(again, first);

        let second = repository.register("relay-b", "iPad", "ios").await.unwrap();
        let listed: Vec<String> = repository
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .filter(|id| *id == first.id || *id == second.id)
            .collect();
        assert_eq!(listed, vec![first.id.clone(), second.id.clone()]);

        repository.mark_success(&first.id).await.unwrap();
        let marked = repository.get(&first.id).await.unwrap().unwrap();
        assert!(marked.last_success_at.is_some());
        assert_eq!(repository.get("missing").await.unwrap(), None);

        assert!(repository.delete(&first.id).await.unwrap());
        assert!(!repository.delete(&first.id).await.unwrap());
        assert_eq!(repository.get(&first.id).await.unwrap(), None);
        assert!(repository.delete(&second.id).await.unwrap());
    }

    #[tokio::test]
    async fn sqlite_subscriptions_register_idempotently_and_can_be_marked_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_database(dir.path().join("push.db")).await.unwrap();

        exercise(&PushRepository::new(DatabasePool::Sqlite(pool))).await;
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn postgres_subscriptions_register_idempotently_and_can_be_marked_and_deleted() {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
        let pool = crate::infrastructure::database::init_postgres_database(&url)
            .await
            .unwrap();

        exercise(&PushRepository::new(DatabasePool::Postgres(pool))).await;
    }

    #[tokio::test]
    async fn imported_subscriptions_keep_their_ids_capabilities_and_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let source = PushRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("source.db")).await.unwrap(),
        ));
        let target = PushRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("target.db")).await.unwrap(),
        ));
        source.register("relay-a", "iPhone", "ios").await.unwrap();
        let paired = source.register("relay-b", "iPad", "ios").await.unwrap();
        source.mark_success(&paired.id).await.unwrap();
        let exported = source.list().await.unwrap();

        target.import(&exported).await.unwrap();
        target.import(&exported).await.unwrap();

        assert_eq!(target.list().await.unwrap(), exported);
        assert_eq!(target.delete_all().await.unwrap(), 2);
        assert!(target.list().await.unwrap().is_empty());
    }
}
