use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};

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

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseError(error.to_string())
}

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

    fn placeholder(&self, index: usize) -> String {
        match &self.pool {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${index}"),
        }
    }

    pub async fn insert(&self, device: &PairedDevice) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO paired_devices ({COLUMNS}) VALUES ({}, {}, {}, {}, {}, {})",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
        );
        self.execute_insert(sql, device).await
    }

    pub async fn list(&self) -> DomainResult<Vec<PairedDevice>> {
        let sql = format!("SELECT {COLUMNS} FROM paired_devices ORDER BY created_at, id");
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

        Ok(rows.into_iter().map(device).collect())
    }

    pub async fn find_by_token(&self, token: &str) -> DomainResult<Option<PairedDevice>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM paired_devices WHERE token_hash = {}",
            self.placeholder(1)
        );
        let token_hash = PairedDevice::hash_token(token);
        let row = match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
                    .bind(token_hash)
                    .fetch_optional(p)
                    .await
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql))
                    .bind(token_hash)
                    .fetch_optional(p)
                    .await
            }
        }
        .map_err(db_error)?;

        Ok(row.map(device))
    }

    pub async fn touch(&self, id: &str) -> DomainResult<()> {
        let sql = format!(
            "UPDATE paired_devices SET last_seen_at = {} WHERE id = {}",
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

    pub async fn set_notification_preferences(
        &self, id: &str, preferences: &NotificationPreferences,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE paired_devices SET notification_preferences = {} WHERE id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let json = serde_json::to_string(preferences)
            .map_err(|error| DomainError::DatabaseError(error.to_string()))?;
        let updated = match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(json)
                .bind(id)
                .execute(p)
                .await
                .map(|result| result.rows_affected()),
            DatabasePool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(json)
                .bind(id)
                .execute(p)
                .await
                .map(|result| result.rows_affected()),
        }
        .map_err(db_error)?;

        Ok(updated > 0)
    }

    pub async fn notification_preferences(
        &self,
    ) -> DomainResult<HashMap<String, NotificationPreferences>> {
        let sql = "SELECT id, notification_preferences FROM paired_devices \
                   WHERE notification_preferences IS NOT NULL";
        let rows = match &self.pool {
            DatabasePool::Sqlite(p) => {
                sqlx::query_as::<_, (String, String)>(sql)
                    .fetch_all(p)
                    .await
            }
            DatabasePool::Postgres(p) => {
                sqlx::query_as::<_, (String, String)>(sql)
                    .fetch_all(p)
                    .await
            }
        }
        .map_err(db_error)?;

        Ok(rows
            .into_iter()
            .filter_map(|(id, json)| Some((id, serde_json::from_str(&json).ok()?)))
            .collect())
    }

    pub async fn delete(&self, id: &str) -> DomainResult<bool> {
        let sql = format!(
            "DELETE FROM paired_devices WHERE id = {}",
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

    pub async fn delete_all(&self) -> DomainResult<u64> {
        let sql = "DELETE FROM paired_devices";
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

    pub async fn import(&self, devices: &[PairedDevice]) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO paired_devices ({COLUMNS}) VALUES ({}, {}, {}, {}, {}, {})
             ON CONFLICT (id) DO NOTHING",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
        );
        for device in devices {
            self.execute_insert(sql.clone(), device).await?;
        }

        Ok(())
    }

    async fn execute_insert(&self, sql: String, device: &PairedDevice) -> DomainResult<()> {
        match &self.pool {
            DatabasePool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&device.id)
                .bind(&device.token_hash)
                .bind(&device.name)
                .bind(&device.platform)
                .bind(device.created_at)
                .bind(device.last_seen_at)
                .execute(p)
                .await
                .map(|_| ()),
            DatabasePool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&device.id)
                .bind(&device.token_hash)
                .bind(&device.name)
                .bind(&device.platform)
                .bind(device.created_at)
                .bind(device.last_seen_at)
                .execute(p)
                .await
                .map(|_| ()),
        }
        .map_err(db_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::init_database;

    #[tokio::test]
    async fn sqlite_devices_are_found_by_token_touched_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_database(dir.path().join("devices.db")).await.unwrap();
        let repository = DeviceRepository::new(DatabasePool::Sqlite(pool));
        let (device, token) = PairedDevice::issue("Pixel", "android");

        repository.insert(&device).await.unwrap();

        let found = repository.find_by_token(&token).await.unwrap().unwrap();
        assert_eq!(found.id, device.id);
        assert!(found.last_seen_at.is_some());
        assert_eq!(repository.find_by_token("wrong").await.unwrap(), None);
        assert_eq!(
            repository.find_by_token(&device.token_hash).await.unwrap(),
            None
        );

        assert!(repository
            .notification_preferences()
            .await
            .unwrap()
            .is_empty());
        let muted = NotificationPreferences {
            long_run: false,
            muted_pipelines: vec!["github__1__api".into()],
            ..NotificationPreferences::default()
        };
        assert!(repository
            .set_notification_preferences(&device.id, &muted)
            .await
            .unwrap());
        assert!(!repository
            .set_notification_preferences("unknown", &muted)
            .await
            .unwrap());
        assert_eq!(
            repository.notification_preferences().await.unwrap(),
            HashMap::from([(device.id.clone(), muted)])
        );

        repository.touch(&device.id).await.unwrap();
        let listed = repository.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].last_seen_at.is_some());

        assert!(repository.delete(&device.id).await.unwrap());
        assert!(!repository.delete(&device.id).await.unwrap());
        assert_eq!(repository.find_by_token(&token).await.unwrap(), None);

        repository.import(&listed).await.unwrap();
        repository.import(&listed).await.unwrap();
        assert_eq!(repository.list().await.unwrap(), listed);
        assert_eq!(repository.delete_all().await.unwrap(), 1);
        assert!(repository.list().await.unwrap().is_empty());
    }
}
