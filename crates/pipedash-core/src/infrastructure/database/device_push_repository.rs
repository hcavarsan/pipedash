use chrono::{
    DateTime,
    Utc,
};

use super::DatabasePool;
use crate::domain::{
    DevicePush,
    DomainError,
    DomainResult,
};

type Row = (
    String,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<i64>,
);

const COLUMNS: &str =
    "device_id, relay_subscription_id, created_at, last_success_at, undelivered_seq";

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseError(error.to_string())
}

fn push(row: Row) -> DevicePush {
    let (device_id, relay_subscription_id, created_at, last_success_at, undelivered_seq) = row;

    DevicePush {
        device_id,
        relay_subscription_id,
        created_at,
        last_success_at,
        undelivered_seq,
    }
}

pub struct DevicePushRepository {
    pool: DatabasePool,
}

impl DevicePushRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn placeholder(&self, index: usize) -> String {
        match &self.pool {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${index}"),
        }
    }

    pub async fn set(&self, device_id: &str, relay_subscription_id: &str) -> DomainResult<()> {
        self.evict(device_id, relay_subscription_id).await?;

        let upsert = format!(
            "INSERT INTO device_push_subscriptions (device_id, relay_subscription_id, created_at)
             VALUES ({}, {}, {})
             ON CONFLICT (device_id) DO UPDATE SET
                 relay_subscription_id = excluded.relay_subscription_id,
                 last_success_at = NULL",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3)
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(upsert))
            .bind(device_id)
            .bind(relay_subscription_id)
            .bind(Utc::now())
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    pub async fn get(&self, device_id: &str) -> DomainResult<Option<DevicePush>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_push_subscriptions WHERE device_id = {}",
            self.placeholder(1)
        );
        let row = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(device_id)
            .fetch_optional(p)
            .await)
        .map_err(db_error)?;

        Ok(row.map(push))
    }

    pub async fn list(&self) -> DomainResult<Vec<DevicePush>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_push_subscriptions ORDER BY created_at, device_id"
        );
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .fetch_all(p)
            .await)
        .map_err(db_error)?;

        Ok(rows.into_iter().map(push).collect())
    }

    pub async fn delete(&self, device_id: &str) -> DomainResult<bool> {
        let sql = format!(
            "DELETE FROM device_push_subscriptions WHERE device_id = {}",
            self.placeholder(1)
        );
        let deleted = on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(device_id)
            .execute(p)
            .await
            .map(|result| result.rows_affected()))
        .map_err(db_error)?;

        Ok(deleted > 0)
    }

    pub async fn delete_matching(
        &self, device_id: &str, relay_subscription_id: &str,
    ) -> DomainResult<bool> {
        let sql = format!(
            "DELETE FROM device_push_subscriptions WHERE device_id = {} AND relay_subscription_id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let deleted = on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(device_id)
            .bind(relay_subscription_id)
            .execute(p)
            .await
            .map(|result| result.rows_affected()))
        .map_err(db_error)?;

        Ok(deleted > 0)
    }

    pub async fn mark_success(&self, device_id: &str) -> DomainResult<()> {
        let sql = format!(
            "UPDATE device_push_subscriptions SET last_success_at = {} WHERE device_id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(Utc::now())
            .bind(device_id)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    pub async fn delete_all(&self) -> DomainResult<u64> {
        let sql = "DELETE FROM device_push_subscriptions";
        on_pool!(&self.pool, p => sqlx::query(sql)
            .execute(p)
            .await
            .map(|result| result.rows_affected()))
        .map_err(db_error)
    }

    pub async fn import(&self, pushes: &[DevicePush]) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO device_push_subscriptions ({COLUMNS}) VALUES ({}, {}, {}, {}, {})
             ON CONFLICT (device_id) DO UPDATE SET
                 relay_subscription_id = excluded.relay_subscription_id,
                 created_at = excluded.created_at,
                 last_success_at = excluded.last_success_at,
                 undelivered_seq = excluded.undelivered_seq",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5)
        );
        for push in pushes {
            self.evict(&push.device_id, &push.relay_subscription_id)
                .await?;
            on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                .bind(&push.device_id)
                .bind(&push.relay_subscription_id)
                .bind(push.created_at)
                .bind(push.last_success_at)
                .bind(push.undelivered_seq)
                .execute(p)
                .await
                .map(|_| ()))
            .map_err(db_error)?;
        }

        Ok(())
    }

    pub async fn mark_undelivered(
        &self, device_id: &str, relay_subscription_id: &str, seq: i64,
    ) -> DomainResult<()> {
        let sql = format!(
            "UPDATE device_push_subscriptions
             SET undelivered_seq = CASE
                 WHEN undelivered_seq IS NULL OR undelivered_seq > {} THEN {}
                 ELSE undelivered_seq
             END
             WHERE device_id = {} AND relay_subscription_id = {}",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4)
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(seq)
            .bind(seq)
            .bind(device_id)
            .bind(relay_subscription_id)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    pub async fn ack_undelivered(&self, device_id: &str, through: i64) -> DomainResult<()> {
        let sql = format!(
            "UPDATE device_push_subscriptions SET undelivered_seq = NULL
             WHERE device_id = {} AND undelivered_seq <= {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(device_id)
            .bind(through)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    async fn evict(&self, device_id: &str, relay_subscription_id: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM device_push_subscriptions WHERE relay_subscription_id = {} AND device_id <> {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(relay_subscription_id)
            .bind(device_id)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::init_database;

    #[tokio::test]
    async fn sqlite_pushes_upsert_evict_duplicates_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let repository = DevicePushRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("pushes.db")).await.unwrap(),
        ));

        repository.set("d1", "sub-1").await.unwrap();
        repository.mark_success("d1").await.unwrap();
        assert!(repository
            .get("d1")
            .await
            .unwrap()
            .unwrap()
            .last_success_at
            .is_some());

        repository.set("d1", "sub-2").await.unwrap();
        let replaced = repository.get("d1").await.unwrap().unwrap();
        assert_eq!(replaced.relay_subscription_id, "sub-2");
        assert_eq!(replaced.last_success_at, None);

        repository.set("d2", "sub-2").await.unwrap();
        assert_eq!(repository.get("d1").await.unwrap(), None);
        let listed = repository.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].device_id, "d2");

        repository.import(&listed).await.unwrap();
        repository.import(&listed).await.unwrap();
        assert_eq!(repository.list().await.unwrap(), listed);

        assert!(repository.delete("d2").await.unwrap());
        assert!(!repository.delete("d2").await.unwrap());
        repository.set("d3", "sub-3").await.unwrap();
        assert_eq!(repository.delete_all().await.unwrap(), 1);
        assert!(repository.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn sqlite_delete_matching_needs_both_ids_to_match() {
        let dir = tempfile::tempdir().unwrap();
        let repository = DevicePushRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("matching.db")).await.unwrap(),
        ));
        repository.set("d1", "sub-1").await.unwrap();

        assert!(!repository.delete_matching("d1", "sub-2").await.unwrap());
        assert!(!repository.delete_matching("d2", "sub-1").await.unwrap());
        assert!(repository.get("d1").await.unwrap().is_some());
        assert!(repository.delete_matching("d1", "sub-1").await.unwrap());
        assert_eq!(repository.get("d1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn sqlite_undelivered_seq_keeps_the_lowest_and_clears_through_the_acked_one() {
        let dir = tempfile::tempdir().unwrap();
        let repository = DevicePushRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("undelivered.db"))
                .await
                .unwrap(),
        ));
        async fn seq(repository: &DevicePushRepository) -> Option<i64> {
            repository.list().await.unwrap()[0].undelivered_seq
        }
        repository.set("d1", "sub-1").await.unwrap();
        assert_eq!(seq(&repository).await, None);

        repository.mark_undelivered("d1", "other", 5).await.unwrap();
        repository.mark_undelivered("d2", "sub-1", 5).await.unwrap();
        assert_eq!(seq(&repository).await, None);

        repository.mark_undelivered("d1", "sub-1", 7).await.unwrap();
        repository.mark_undelivered("d1", "sub-1", 9).await.unwrap();
        repository.mark_undelivered("d1", "sub-1", 4).await.unwrap();
        assert_eq!(seq(&repository).await, Some(4));

        repository.set("d1", "sub-2").await.unwrap();
        assert_eq!(seq(&repository).await, Some(4));

        repository.ack_undelivered("d2", 100).await.unwrap();
        repository.ack_undelivered("d1", 3).await.unwrap();
        assert_eq!(seq(&repository).await, Some(4));
        repository.ack_undelivered("d1", 4).await.unwrap();
        assert_eq!(seq(&repository).await, None);

        repository.mark_undelivered("d1", "sub-2", 8).await.unwrap();
        let exported = repository.list().await.unwrap();
        repository.delete_all().await.unwrap();
        repository.import(&exported).await.unwrap();
        assert_eq!(seq(&repository).await, Some(8));
    }

    #[tokio::test]
    async fn sqlite_import_overwrites_a_stale_registration_for_the_same_device() {
        let dir = tempfile::tempdir().unwrap();
        let source = DevicePushRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("source.db")).await.unwrap(),
        ));
        let target = DevicePushRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("target.db")).await.unwrap(),
        ));
        target.set("d1", "stale").await.unwrap();
        target.set("d2", "taken").await.unwrap();
        source.set("d1", "fresh").await.unwrap();
        source.set("d3", "taken").await.unwrap();

        target.import(&source.list().await.unwrap()).await.unwrap();

        assert_eq!(
            target
                .get("d1")
                .await
                .unwrap()
                .unwrap()
                .relay_subscription_id,
            "fresh"
        );
        assert_eq!(target.get("d2").await.unwrap(), None);
        assert_eq!(
            target
                .get("d3")
                .await
                .unwrap()
                .unwrap()
                .relay_subscription_id,
            "taken"
        );
    }
}
