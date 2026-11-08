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
use super::DatabasePool;
use crate::domain::{
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

    pub async fn list(&self) -> DomainResult<Vec<PushSubscription>> {
        let sql = format!("SELECT {COLUMNS} FROM push_subscriptions ORDER BY created_at, id");
        let rows: Vec<Row> = fetch_all_as(&self.pool, &sql, &[]).await?;

        Ok(rows.into_iter().map(subscription).collect())
    }

    pub async fn get(&self, id: &str) -> DomainResult<Option<PushSubscription>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM push_subscriptions WHERE id = {}",
            self.pool.placeholder(1)
        );
        let row: Option<Row> = fetch_optional_as(&self.pool, &sql, &[BindValue::Text(id)]).await?;

        Ok(row.map(subscription))
    }

    pub async fn delete(&self, id: &str) -> DomainResult<bool> {
        delete_by_key(&self.pool, "push_subscriptions", "id", id).await
    }

    pub async fn mark_success(&self, id: &str) -> DomainResult<()> {
        set_timestamp(
            &self.pool,
            "push_subscriptions",
            "last_success_at",
            "id",
            id,
            Utc::now(),
        )
        .await
    }

    pub async fn delete_all(&self) -> DomainResult<u64> {
        execute_bound(&self.pool, "DELETE FROM push_subscriptions", &[]).await
    }

    pub async fn import(&self, subscriptions: &[PushSubscription]) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO push_subscriptions
                 (id, relay_subscription_id, device_name, platform, created_at, last_success_at)
             VALUES ({})
             ON CONFLICT (id) DO UPDATE SET
                 relay_subscription_id = excluded.relay_subscription_id,
                 device_name = excluded.device_name,
                 platform = excluded.platform,
                 created_at = excluded.created_at,
                 last_success_at = excluded.last_success_at",
            self.pool.placeholders(6),
        );

        for subscription in subscriptions {
            let values = [
                BindValue::Text(&subscription.id),
                BindValue::Text(&subscription.relay_subscription_id),
                BindValue::Text(&subscription.device_name),
                BindValue::Text(&subscription.platform),
                BindValue::Timestamp(subscription.created_at),
                BindValue::OptTimestamp(subscription.last_success_at),
            ];
            execute_bound(&self.pool, &sql, &values).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
