use chrono::{
    DateTime,
    Duration,
    Utc,
};

use super::DatabasePool;
use crate::domain::{
    new_alert_id,
    AlertKind,
    DeviceAlert,
    DomainError,
    DomainResult,
    NewDeviceAlert,
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
    DateTime<Utc>,
);

const COLUMNS: &str = "seq, id, alert_key, kind, title, body, pipeline_id, provider_id, created_at";
const RETENTION_DAYS: i64 = 7;
const MAX_ROWS: i64 = 500;

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseError(error.to_string())
}

fn alert(row: Row) -> DomainResult<DeviceAlert> {
    let (seq, id, key, kind, title, body, pipeline_id, provider_id, created_at) = row;
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

    fn placeholder(&self, index: usize) -> String {
        match &self.pool {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${index}"),
        }
    }

    fn placeholders(&self, count: usize) -> String {
        (1..=count)
            .map(|index| self.placeholder(index))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub async fn insert(&self, input: &NewDeviceAlert) -> DomainResult<DeviceAlert> {
        let sql = format!(
            "INSERT INTO device_alerts (id, alert_key, kind, title, body, pipeline_id, provider_id, created_at)
             VALUES ({}) RETURNING {COLUMNS}",
            self.placeholders(8)
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
            .bind(Utc::now())
            .fetch_one(p)
            .await)
        .map_err(db_error)?;

        if let Err(error) = self.prune().await {
            tracing::warn!(%error, "Failed to prune old device alerts");
        }

        alert(row)
    }

    async fn prune(&self) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM device_alerts
             WHERE created_at < {} OR seq <= (SELECT MAX(seq) FROM device_alerts) - {MAX_ROWS}",
            self.placeholders(1)
        );
        let cutoff = Utc::now() - Duration::days(RETENTION_DAYS);
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(cutoff)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    pub async fn after(&self, after: i64, limit: i64) -> DomainResult<Vec<DeviceAlert>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_alerts WHERE seq > {} AND kind <> 'test'
             ORDER BY seq ASC LIMIT {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(after)
            .bind(limit)
            .fetch_all(p)
            .await)
        .map_err(db_error)?;

        alerts(rows)
    }

    pub async fn latest_seq(&self) -> DomainResult<i64> {
        let sql = "SELECT COALESCE(MAX(seq), 0) FROM device_alerts";
        on_pool!(&self.pool, p => sqlx::query_scalar::<_, i64>(sql).fetch_one(p).await)
            .map_err(db_error)
    }

    pub async fn get(&self, id: &str) -> DomainResult<Option<DeviceAlert>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_alerts WHERE id = {}",
            self.placeholders(1)
        );
        let row = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(p)
            .await)
        .map_err(db_error)?;

        row.map(alert).transpose()
    }

    pub async fn list(&self) -> DomainResult<Vec<DeviceAlert>> {
        let sql = format!("SELECT {COLUMNS} FROM device_alerts ORDER BY seq");
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .fetch_all(p)
            .await)
        .map_err(db_error)?;

        alerts(rows)
    }

    pub async fn import(&self, imported: &[DeviceAlert]) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO device_alerts ({COLUMNS}) VALUES ({}) ON CONFLICT DO NOTHING",
            self.placeholders(9)
        );
        for alert in imported {
            on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                .bind(alert.seq)
                .bind(&alert.id)
                .bind(&alert.key)
                .bind(alert.kind.as_str())
                .bind(&alert.title)
                .bind(&alert.body)
                .bind(alert.pipeline_id.as_deref())
                .bind(alert.provider_id)
                .bind(alert.created_at)
                .execute(p)
                .await
                .map(|_| ()))
            .map_err(db_error)?;
        }

        if let DatabasePool::Postgres(p) = &self.pool {
            sqlx::query(
                "SELECT setval(pg_get_serial_sequence('device_alerts', 'seq'),
                               (SELECT COALESCE(MAX(seq), 1) FROM device_alerts))",
            )
            .execute(p)
            .await
            .map_err(db_error)?;
        }

        Ok(())
    }

    pub async fn delete_all(&self) -> DomainResult<u64> {
        let sql = "DELETE FROM device_alerts";
        on_pool!(&self.pool, p => sqlx::query(sql)
            .execute(p)
            .await
            .map(|result| result.rows_affected()))
        .map_err(db_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::init_database;

    fn new_alert(key: &str, kind: AlertKind) -> NewDeviceAlert {
        NewDeviceAlert {
            key: key.to_string(),
            kind,
            title: format!("{key} title"),
            body: "body".to_string(),
            pipeline_id: Some(key.to_string()),
            provider_id: Some(7),
        }
    }

    async fn repository(dir: &std::path::Path, name: &str) -> DeviceAlertRepository {
        DeviceAlertRepository::new(DatabasePool::Sqlite(
            init_database(dir.join(name)).await.unwrap(),
        ))
    }

    #[tokio::test]
    async fn sqlite_alerts_page_by_sequence_and_hide_test_alerts() {
        let dir = tempfile::tempdir().unwrap();
        let repository = repository(dir.path(), "alerts.db").await;
        assert_eq!(repository.latest_seq().await.unwrap(), 0);

        let first = repository
            .insert(&new_alert("a", AlertKind::Failed))
            .await
            .unwrap();
        let test = repository
            .insert(&new_alert("t", AlertKind::Test))
            .await
            .unwrap();
        let last = repository
            .insert(&new_alert("b", AlertKind::LongRun))
            .await
            .unwrap();

        assert_eq!((first.seq, test.seq, last.seq), (1, 2, 3));
        assert_eq!(first.id.len(), 32);
        assert_eq!(repository.latest_seq().await.unwrap(), 3);
        assert_eq!(
            repository.after(0, 10).await.unwrap(),
            vec![first.clone(), last.clone()]
        );
        assert_eq!(repository.after(1, 10).await.unwrap(), vec![last.clone()]);
        assert_eq!(repository.after(0, 1).await.unwrap(), vec![first.clone()]);
        assert_eq!(repository.get(&test.id).await.unwrap(), Some(test));
        assert_eq!(repository.get("missing").await.unwrap(), None);
    }

    #[tokio::test]
    async fn sqlite_alerts_keep_at_most_500_rows_and_drop_week_old_ones() {
        let dir = tempfile::tempdir().unwrap();
        let repository = repository(dir.path(), "prune.db").await;
        for index in 0..505 {
            repository
                .insert(&new_alert(&format!("p{index}"), AlertKind::Failed))
                .await
                .unwrap();
        }

        let kept = repository.list().await.unwrap();
        assert_eq!(kept.len(), 500);
        assert_eq!(kept[0].seq, 6);
        assert_eq!(kept[499].seq, 505);

        let pool = match &repository.pool {
            DatabasePool::Sqlite(pool) => pool,
            DatabasePool::Postgres(_) => unreachable!(),
        };
        let old = (Utc::now() - Duration::days(8)).to_rfc3339();
        sqlx::query("UPDATE device_alerts SET created_at = ? WHERE seq <= 10")
            .bind(old)
            .execute(pool)
            .await
            .unwrap();
        repository
            .insert(&new_alert("fresh", AlertKind::Failed))
            .await
            .unwrap();

        let kept = repository.list().await.unwrap();
        assert_eq!(kept.len(), 496);
        assert_eq!(kept[0].seq, 11);
    }

    async fn assert_import_keeps_sequence(
        source: &DeviceAlertRepository, target: &DeviceAlertRepository,
    ) {
        source.delete_all().await.unwrap();
        target.delete_all().await.unwrap();
        for key in ["a", "b", "c"] {
            source
                .insert(&new_alert(key, AlertKind::Failed))
                .await
                .unwrap();
        }
        let exported = source.list().await.unwrap();

        target.import(&exported).await.unwrap();
        target.import(&exported).await.unwrap();

        assert_eq!(target.list().await.unwrap(), exported);
        let next = target
            .insert(&new_alert("d", AlertKind::Failed))
            .await
            .unwrap();
        assert_eq!(next.seq, exported.last().unwrap().seq + 1);
    }

    #[tokio::test]
    async fn sqlite_imported_alerts_keep_their_sequence_and_the_next_insert_follows() {
        let dir = tempfile::tempdir().unwrap();
        let source = repository(dir.path(), "source.db").await;
        let target = repository(dir.path(), "target.db").await;
        for key in ["x", "y"] {
            source
                .insert(&new_alert(key, AlertKind::Failed))
                .await
                .unwrap();
        }
        source.delete_all().await.unwrap();

        assert_import_keeps_sequence(&source, &target).await;
        let kept = source.list().await.unwrap();
        assert_eq!(kept.first().map(|alert| alert.seq), Some(3));
        assert_eq!(target.delete_all().await.unwrap(), 4);
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn postgres_imported_alerts_keep_their_sequence_and_the_next_insert_follows() {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
        let pool = crate::infrastructure::database::init_postgres_database(&url)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let source = repository(dir.path(), "source.db").await;
        let target = DeviceAlertRepository::new(DatabasePool::Postgres(pool));

        assert_import_keeps_sequence(&source, &target).await;
    }
}
