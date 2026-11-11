use chrono::{
    DateTime,
    Duration,
    SubsecRound,
    Utc,
};

use super::pool::db_err;
use super::DatabasePool;
use crate::domain::{
    new_alert_id,
    AlertKind,
    DeviceAlert,
    DomainError,
    DomainResult,
    NewDeviceAlert,
    RunFinish,
};

type Row = (
    i64,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
);

const COLUMNS: &str = "seq, id, alert_key, kind, title, body, pipeline_id, provider_id, \
                       provider_ids, finish, created_at";
const RETENTION_DAYS: i64 = 7;
const MAX_ROWS: i64 = 500;

macro_rules! insert_with_seq {
    ($sql:expr, $alert:expr, $executor:expr) => {
        sqlx::query(sqlx::AssertSqlSafe($sql.clone()))
            .bind($alert.seq)
            .bind(&$alert.id)
            .bind(&$alert.key)
            .bind($alert.kind.as_str())
            .bind(&$alert.title)
            .bind(&$alert.body)
            .bind($alert.pipeline_id.as_deref())
            .bind($alert.provider_id)
            .bind(provider_ids_json(&$alert.provider_ids))
            .bind(finish_json($alert.finish.as_ref()))
            .bind($alert.created_at)
            .execute($executor)
            .await
            .map(|_| ())
    };
}

fn cutoff() -> DateTime<Utc> {
    Utc::now() - Duration::days(RETENTION_DAYS)
}

fn provider_ids_json(ids: &[i64]) -> Option<String> {
    (!ids.is_empty()).then(|| serde_json::to_string(ids).unwrap_or_default())
}

fn finish_json(finish: Option<&RunFinish>) -> Option<String> {
    finish.and_then(|finish| serde_json::to_string(finish).ok())
}

fn alert(row: Row) -> DomainResult<DeviceAlert> {
    let (
        seq,
        id,
        key,
        kind,
        title,
        body,
        pipeline_id,
        provider_id,
        provider_ids,
        finish,
        created_at,
    ) = row;
    let provider_ids = provider_ids
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let finish = finish.and_then(|json| serde_json::from_str(&json).ok());
    let kind = AlertKind::parse(&kind)
        .ok_or_else(|| DomainError::DatabaseError(format!("Unknown alert kind: {kind}")))?;

    Ok(DeviceAlert {
        seq,
        id,
        key,
        kind,
        title,
        body,
        pipeline_id,
        provider_id,
        provider_ids,
        finish,
        created_at,
    })
}

fn alerts(rows: Vec<Row>) -> DomainResult<Vec<DeviceAlert>> {
    rows.into_iter().map(alert).collect()
}

pub struct DeviceAlertRepository {
    pool: DatabasePool,
}

impl DeviceAlertRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, input: &NewDeviceAlert) -> DomainResult<DeviceAlert> {
        let sql = format!(
            "INSERT INTO device_alerts (id, alert_key, kind, title, body, pipeline_id, provider_id, provider_ids, finish, created_at)
             VALUES ({}) RETURNING {COLUMNS}",
            self.pool.placeholders(10)
        );
        let id = new_alert_id();
        let row = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(&id)
            .bind(&input.key)
            .bind(input.kind.as_str())
            .bind(&input.title)
            .bind(&input.body)
            .bind(input.pipeline_id.as_deref())
            .bind(input.provider_id)
            .bind(provider_ids_json(&input.provider_ids))
            .bind(finish_json(input.finish.as_ref()))
            .bind(Utc::now().trunc_subsecs(6))
            .fetch_one(p)
            .await)
        .map_err(db_err)?;

        if let Err(error) = self.prune().await {
            tracing::warn!(%error, "Failed to prune old device alerts");
        }

        alert(row)
    }

    pub async fn prune(&self) -> DomainResult<u64> {
        let sql = format!(
            "DELETE FROM device_alerts
             WHERE created_at < {} OR seq <= (SELECT MAX(seq) FROM device_alerts) - {MAX_ROWS}",
            self.pool.placeholder(1)
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(cutoff())
            .execute(p)
            .await
            .map(|result| result.rows_affected()))
        .map_err(db_err)
    }

    pub async fn after(&self, after: i64, limit: i64) -> DomainResult<Vec<DeviceAlert>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_alerts
             WHERE seq > {} AND kind <> 'test' AND created_at >= {}
             ORDER BY seq ASC LIMIT {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3)
        );
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(after)
            .bind(cutoff())
            .bind(limit)
            .fetch_all(p)
            .await)
        .map_err(db_err)?;

        alerts(rows)
    }

    pub async fn latest_seq(&self) -> DomainResult<i64> {
        let sql = format!(
            "SELECT COALESCE(MAX(seq), 0) FROM device_alerts WHERE created_at >= {}",
            self.pool.placeholder(1)
        );
        on_pool!(&self.pool, p => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .bind(cutoff())
            .fetch_one(p)
            .await)
        .map_err(db_err)
    }

    pub async fn get(&self, id: &str) -> DomainResult<Option<DeviceAlert>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_alerts WHERE id = {} AND created_at >= {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let row = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(cutoff())
            .fetch_optional(p)
            .await)
        .map_err(db_err)?;

        row.map(alert).transpose()
    }

    pub async fn list(&self) -> DomainResult<Vec<DeviceAlert>> {
        let sql = format!("SELECT {COLUMNS} FROM device_alerts ORDER BY seq");
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .fetch_all(p)
            .await)
        .map_err(db_err)?;

        alerts(rows)
    }

    pub async fn import(&self, imported: &[DeviceAlert]) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO device_alerts ({COLUMNS}) VALUES ({}) ON CONFLICT DO NOTHING",
            self.pool.placeholders(11)
        );
        for alert in imported {
            on_pool!(&self.pool, p => insert_with_seq!(sql, alert, p)).map_err(db_err)?;
        }

        self.sync_sequence().await
    }

    pub async fn merge(&self, incoming: &[DeviceAlert]) -> DomainResult<()> {
        if incoming.is_empty() {
            return Ok(());
        }

        let mut incoming: Vec<&DeviceAlert> = incoming.iter().collect();
        incoming.sort_by_key(|alert| alert.seq);
        let highest_incoming = incoming.iter().map(|alert| alert.seq).max().unwrap_or(0);
        let highest = "SELECT COALESCE(MAX(seq), 0) FROM device_alerts";
        let by_id = format!(
            "SELECT seq FROM device_alerts WHERE id = {}",
            self.pool.placeholder(1)
        );
        let by_seq = format!(
            "SELECT seq FROM device_alerts WHERE seq = {}",
            self.pool.placeholder(1)
        );
        let relocate = format!(
            "UPDATE device_alerts SET seq = {} WHERE seq = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let insert = format!(
            "INSERT INTO device_alerts ({COLUMNS}) VALUES ({})",
            self.pool.placeholders(11)
        );

        on_pool!(&self.pool, p => async {
            let mut tx = p.begin().await?;
            let mut spare = sqlx::query_scalar::<_, i64>(highest)
                .fetch_one(&mut *tx)
                .await?
                .max(highest_incoming);
            for alert in &incoming {
                let known = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(by_id.clone()))
                    .bind(&alert.id)
                    .fetch_optional(&mut *tx)
                    .await?;
                if known.is_some() {
                    continue;
                }

                let taken = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(by_seq.clone()))
                    .bind(alert.seq)
                    .fetch_optional(&mut *tx)
                    .await?;
                if taken.is_some() {
                    spare += 1;
                    sqlx::query(sqlx::AssertSqlSafe(relocate.clone()))
                        .bind(spare)
                        .bind(alert.seq)
                        .execute(&mut *tx)
                        .await?;
                }

                insert_with_seq!(insert, alert, &mut *tx)?;
            }
            tx.commit().await
        }
        .await)
        .map_err(db_err)?;

        self.sync_sequence().await
    }

    async fn sync_sequence(&self) -> DomainResult<()> {
        if let DatabasePool::Postgres(p) = &self.pool {
            sqlx::query(
                "SELECT setval(pg_get_serial_sequence('device_alerts', 'seq'),
                               (SELECT COALESCE(MAX(seq), 1) FROM device_alerts))",
            )
            .execute(p)
            .await
            .map_err(db_err)?;
        }

        Ok(())
    }

    pub async fn delete_all(&self) -> DomainResult<u64> {
        let sql = "DELETE FROM device_alerts";
        on_pool!(&self.pool, p => sqlx::query(sql)
            .execute(p)
            .await
            .map(|result| result.rows_affected()))
        .map_err(db_err)
    }
}

#[cfg(test)]
mod tests;
