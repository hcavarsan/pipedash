use chrono::{
    DateTime,
    SubsecRound,
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
use super::DatabasePool;
use crate::domain::{
    DevicePush,
    DomainResult,
};

type Row = (
    String,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<i64>,
    Option<i64>,
);

const COLUMNS: &str = "device_id, relay_subscription_id, created_at, last_success_at, \
                       undelivered_seq, undelivered_max_seq";

fn push(row: Row) -> DevicePush {
    let (
        device_id,
        relay_subscription_id,
        created_at,
        last_success_at,
        undelivered_seq,
        undelivered_max_seq,
    ) = row;

    DevicePush {
        device_id,
        relay_subscription_id,
        created_at,
        last_success_at,
        undelivered_seq,
        undelivered_max_seq,
    }
}

pub struct DevicePushRepository {
    pool: DatabasePool,
}

impl DevicePushRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub async fn set(&self, device_id: &str, relay_subscription_id: &str) -> DomainResult<()> {
        self.evict(device_id, relay_subscription_id).await?;

        let upsert = format!(
            "INSERT INTO device_push_subscriptions (device_id, relay_subscription_id, created_at)
             VALUES ({}, {}, {})
             ON CONFLICT (device_id) DO UPDATE SET
                 relay_subscription_id = excluded.relay_subscription_id,
                 last_success_at = NULL",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3)
        );
        let values = [
            BindValue::Text(device_id),
            BindValue::Text(relay_subscription_id),
            BindValue::Timestamp(Utc::now().trunc_subsecs(6)),
        ];
        execute_bound(&self.pool, &upsert, &values).await?;

        Ok(())
    }

    pub async fn get(&self, device_id: &str) -> DomainResult<Option<DevicePush>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_push_subscriptions WHERE device_id = {}",
            self.pool.placeholder(1)
        );
        let row: Option<Row> =
            fetch_optional_as(&self.pool, &sql, &[BindValue::Text(device_id)]).await?;

        Ok(row.map(push))
    }

    pub async fn list(&self) -> DomainResult<Vec<DevicePush>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_push_subscriptions ORDER BY created_at, device_id"
        );
        let rows: Vec<Row> = fetch_all_as(&self.pool, &sql, &[]).await?;

        Ok(rows.into_iter().map(push).collect())
    }

    pub async fn delete(&self, device_id: &str) -> DomainResult<bool> {
        delete_by_key(
            &self.pool,
            "device_push_subscriptions",
            "device_id",
            device_id,
        )
        .await
    }

    pub async fn delete_matching(
        &self, device_id: &str, relay_subscription_id: &str,
    ) -> DomainResult<bool> {
        let sql = format!(
            "DELETE FROM device_push_subscriptions WHERE device_id = {} AND relay_subscription_id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let values = [
            BindValue::Text(device_id),
            BindValue::Text(relay_subscription_id),
        ];
        let deleted = execute_bound(&self.pool, &sql, &values).await?;

        Ok(deleted > 0)
    }

    pub async fn mark_success(&self, device_id: &str) -> DomainResult<()> {
        let at = Utc::now().trunc_subsecs(6);
        set_timestamp(
            &self.pool,
            "device_push_subscriptions",
            "last_success_at",
            "device_id",
            device_id,
            at,
        )
        .await
    }

    pub async fn delete_all(&self) -> DomainResult<u64> {
        execute_bound(&self.pool, "DELETE FROM device_push_subscriptions", &[]).await
    }

    pub async fn import(&self, pushes: &[DevicePush]) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO device_push_subscriptions ({COLUMNS}) VALUES ({})
             ON CONFLICT (device_id) DO UPDATE SET
                 relay_subscription_id = excluded.relay_subscription_id,
                 created_at = excluded.created_at,
                 last_success_at = excluded.last_success_at,
                 undelivered_seq = excluded.undelivered_seq,
                 undelivered_max_seq = excluded.undelivered_max_seq",
            self.pool.placeholders(6)
        );
        for push in pushes {
            self.evict(&push.device_id, &push.relay_subscription_id)
                .await?;
            let values = [
                BindValue::Text(&push.device_id),
                BindValue::Text(&push.relay_subscription_id),
                BindValue::Timestamp(push.created_at),
                BindValue::OptTimestamp(push.last_success_at),
                BindValue::OptInt(push.undelivered_seq),
                BindValue::OptInt(push.undelivered_max_seq),
            ];
            execute_bound(&self.pool, &sql, &values).await?;
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
                 END,
                 undelivered_max_seq = CASE
                     WHEN undelivered_max_seq IS NULL OR undelivered_max_seq < {} THEN {}
                     ELSE undelivered_max_seq
                 END
             WHERE device_id = {} AND relay_subscription_id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3),
            self.pool.placeholder(4),
            self.pool.placeholder(5),
            self.pool.placeholder(6)
        );
        let values = [
            BindValue::Int(seq),
            BindValue::Int(seq),
            BindValue::Int(seq),
            BindValue::Int(seq),
            BindValue::Text(device_id),
            BindValue::Text(relay_subscription_id),
        ];
        execute_bound(&self.pool, &sql, &values).await?;

        Ok(())
    }

    pub async fn ack_undelivered(&self, device_id: &str, through: i64) -> DomainResult<()> {
        let sql = format!(
            "UPDATE device_push_subscriptions
             SET undelivered_seq = CASE
                     WHEN undelivered_max_seq <= {} THEN NULL
                     WHEN undelivered_seq <= {} THEN {}
                     ELSE undelivered_seq
                 END,
                 undelivered_max_seq = CASE
                     WHEN undelivered_max_seq <= {} THEN NULL
                     ELSE undelivered_max_seq
                 END
             WHERE device_id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3),
            self.pool.placeholder(4),
            self.pool.placeholder(5)
        );
        let values = [
            BindValue::Int(through),
            BindValue::Int(through),
            BindValue::Int(through.saturating_add(1)),
            BindValue::Int(through),
            BindValue::Text(device_id),
        ];
        execute_bound(&self.pool, &sql, &values).await?;

        Ok(())
    }

    async fn evict(&self, device_id: &str, relay_subscription_id: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM device_push_subscriptions WHERE relay_subscription_id = {} AND device_id <> {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let values = [
            BindValue::Text(relay_subscription_id),
            BindValue::Text(device_id),
        ];
        execute_bound(&self.pool, &sql, &values).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
