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
            .bind($alert.created_at)
            .execute($executor)
            .await
            .map(|_| ())
    };
}

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseError(error.to_string())
}

fn cutoff() -> DateTime<Utc> {
    Utc::now() - Duration::days(RETENTION_DAYS)
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

    pub async fn prune(&self) -> DomainResult<u64> {
        let sql = format!(
            "DELETE FROM device_alerts
             WHERE created_at < {} OR seq <= (SELECT MAX(seq) FROM device_alerts) - {MAX_ROWS}",
            self.placeholder(1)
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(cutoff())
            .execute(p)
            .await
            .map(|result| result.rows_affected()))
        .map_err(db_error)
    }

    pub async fn after(&self, after: i64, limit: i64) -> DomainResult<Vec<DeviceAlert>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_alerts
             WHERE seq > {} AND kind <> 'test' AND created_at >= {}
             ORDER BY seq ASC LIMIT {}",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3)
        );
        let rows = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(after)
            .bind(cutoff())
            .bind(limit)
            .fetch_all(p)
            .await)
        .map_err(db_error)?;

        alerts(rows)
    }

    pub async fn latest_seq(&self) -> DomainResult<i64> {
        let sql = format!(
            "SELECT COALESCE(MAX(seq), 0) FROM device_alerts WHERE created_at >= {}",
            self.placeholder(1)
        );
        on_pool!(&self.pool, p => sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .bind(cutoff())
            .fetch_one(p)
            .await)
        .map_err(db_error)
    }

    pub async fn get(&self, id: &str) -> DomainResult<Option<DeviceAlert>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM device_alerts WHERE id = {} AND created_at >= {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let row = on_pool!(&self.pool, p => sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(cutoff())
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
            on_pool!(&self.pool, p => insert_with_seq!(sql, alert, p)).map_err(db_error)?;
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
            self.placeholder(1)
        );
        let by_seq = format!(
            "SELECT seq FROM device_alerts WHERE seq = {}",
            self.placeholder(1)
        );
        let relocate = format!(
            "UPDATE device_alerts SET seq = {} WHERE seq = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let insert = format!(
            "INSERT INTO device_alerts ({COLUMNS}) VALUES ({})",
            self.placeholders(9)
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
        .map_err(db_error)?;

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
    async fn sqlite_expired_alerts_are_invisible_before_they_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let repository = repository(dir.path(), "expired.db").await;
        let old = repository
            .insert(&new_alert("old", AlertKind::Failed))
            .await
            .unwrap();
        let fresh = repository
            .insert(&new_alert("fresh", AlertKind::Failed))
            .await
            .unwrap();
        let pool = match &repository.pool {
            DatabasePool::Sqlite(pool) => pool,
            DatabasePool::Postgres(_) => unreachable!(),
        };
        sqlx::query("UPDATE device_alerts SET created_at = ? WHERE seq = ?")
            .bind((Utc::now() - Duration::days(8)).to_rfc3339())
            .bind(old.seq)
            .execute(pool)
            .await
            .unwrap();

        assert_eq!(repository.after(0, 10).await.unwrap(), vec![fresh.clone()]);
        assert_eq!(repository.get(&old.id).await.unwrap(), None);
        assert_eq!(
            repository.get(&fresh.id).await.unwrap(),
            Some(fresh.clone())
        );
        assert_eq!(repository.latest_seq().await.unwrap(), fresh.seq);
        assert_eq!(repository.list().await.unwrap().len(), 2);

        assert_eq!(repository.prune().await.unwrap(), 1);
        assert_eq!(repository.list().await.unwrap(), vec![fresh]);

        sqlx::query("UPDATE device_alerts SET created_at = ?")
            .bind((Utc::now() - Duration::days(8)).to_rfc3339())
            .execute(pool)
            .await
            .unwrap();
        assert_eq!(repository.latest_seq().await.unwrap(), 0);
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

    fn stored(seq: i64, id: &str) -> DeviceAlert {
        DeviceAlert {
            seq,
            id: id.to_string(),
            key: id.to_string(),
            kind: AlertKind::Failed,
            title: format!("{id} title"),
            body: "body".to_string(),
            pipeline_id: None,
            provider_id: None,
            created_at: chrono::SubsecRound::trunc_subsecs(Utc::now(), 0),
        }
    }

    fn seqs(alerts: &[DeviceAlert]) -> Vec<(i64, &str)> {
        alerts
            .iter()
            .map(|alert| (alert.seq, alert.id.as_str()))
            .collect()
    }

    async fn assert_merge_keeps_target_alerts_for_an_empty_source(target: &DeviceAlertRepository) {
        target.delete_all().await.unwrap();
        target
            .import(&[stored(1, "a"), stored(2, "b")])
            .await
            .unwrap();
        let before = target.list().await.unwrap();

        target.merge(&[]).await.unwrap();

        assert_eq!(target.list().await.unwrap(), before);
    }

    async fn assert_merge_skips_alerts_the_target_already_has(target: &DeviceAlertRepository) {
        target.delete_all().await.unwrap();
        let source = [stored(1, "a"), stored(2, "b")];
        target.import(&source).await.unwrap();
        target.import(&[stored(3, "c")]).await.unwrap();
        let before = target.list().await.unwrap();

        target.merge(&source).await.unwrap();
        target.merge(&[stored(9, "b")]).await.unwrap();

        assert_eq!(target.list().await.unwrap(), before);
    }

    async fn assert_merge_moves_colliding_target_alerts(target: &DeviceAlertRepository) {
        target.delete_all().await.unwrap();
        target
            .import(&[stored(1, "t1"), stored(2, "t2"), stored(3, "t3")])
            .await
            .unwrap();

        target
            .merge(&[stored(5, "s5"), stored(2, "s2"), stored(3, "s3")])
            .await
            .unwrap();

        let merged = target.list().await.unwrap();
        let expected = vec![
            (1, "t1"),
            (2, "s2"),
            (3, "s3"),
            (5, "s5"),
            (6, "t2"),
            (7, "t3"),
        ];
        assert_eq!(seqs(&merged), expected);
        for (seq, id) in expected {
            assert_eq!(target.get(id).await.unwrap().map(|a| a.seq), Some(seq));
        }

        let next = target
            .insert(&new_alert("next", AlertKind::Failed))
            .await
            .unwrap();
        assert_eq!(next.seq, 8);
    }

    #[tokio::test]
    async fn sqlite_merge_keeps_target_alerts_for_an_empty_source() {
        let dir = tempfile::tempdir().unwrap();
        let target = repository(dir.path(), "empty.db").await;

        assert_merge_keeps_target_alerts_for_an_empty_source(&target).await;
    }

    #[tokio::test]
    async fn sqlite_merge_skips_alerts_the_target_already_has() {
        let dir = tempfile::tempdir().unwrap();
        let target = repository(dir.path(), "known.db").await;

        assert_merge_skips_alerts_the_target_already_has(&target).await;
    }

    #[tokio::test]
    async fn sqlite_merge_moves_colliding_target_alerts_above_both_sets() {
        let dir = tempfile::tempdir().unwrap();
        let target = repository(dir.path(), "collide.db").await;

        assert_merge_moves_colliding_target_alerts(&target).await;
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn postgres_imports_and_merges_keep_sequences_and_the_next_insert_follows() {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
        let pool = crate::infrastructure::database::init_postgres_database(&url)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let source = repository(dir.path(), "source.db").await;
        let target = DeviceAlertRepository::new(DatabasePool::Postgres(pool));

        assert_import_keeps_sequence(&source, &target).await;
        assert_merge_keeps_target_alerts_for_an_empty_source(&target).await;
        assert_merge_skips_alerts_the_target_already_has(&target).await;
        assert_merge_moves_colliding_target_alerts(&target).await;
    }
}
