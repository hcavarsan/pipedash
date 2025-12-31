use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{
    DateTime,
    Utc,
};
use sqlx::postgres::PgPool;
use sqlx::Row;

use super::{
    ObjectMetadata,
    StorageBackend,
};
use crate::domain::{
    DomainError,
    DomainResult,
};

#[derive(Debug, Clone)]
pub struct PostgresCacheConfig {
    pub connection_string: String,
}

pub struct PostgresStorage {
    pool: PgPool,
}

impl PostgresStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn from_connection_string(connection_string: &str) -> DomainResult<Self> {
        use sqlx::postgres::PgPoolOptions;

        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(connection_string)
            .await
            .map_err(|e| {
                DomainError::DatabaseError(format!("Failed to connect to PostgreSQL: {}", e))
            })?;

        Self::init_cache_table(&pool).await?;

        Ok(Self { pool })
    }

    async fn init_cache_table(pool: &PgPool) -> DomainResult<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cache_objects (
                key TEXT PRIMARY KEY,
                data BYTEA NOT NULL,
                size BIGINT NOT NULL,
                content_type TEXT,
                etag TEXT,
                metadata JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to create cache table: {}", e)))?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_cache_objects_key ON cache_objects(key)
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to create cache index: {}", e)))?;

        Ok(())
    }

    fn generate_etag(data: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{
            Hash,
            Hasher,
        };

        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[async_trait]
impl StorageBackend for PostgresStorage {
    fn backend_type(&self) -> &str {
        "postgres"
    }

    async fn is_available(&self) -> bool {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await.is_ok()
    }

    async fn list(&self, prefix: Option<&str>) -> DomainResult<Vec<ObjectMetadata>> {
        let rows = if let Some(prefix) = prefix {
            let pattern = format!("{}%", prefix);
            sqlx::query(
                r#"
                SELECT key, size, content_type, etag, metadata, updated_at
                FROM cache_objects
                WHERE key LIKE $1
                ORDER BY key
                "#,
            )
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                r#"
                SELECT key, size, content_type, etag, metadata, updated_at
                FROM cache_objects
                ORDER BY key
                "#,
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| DomainError::DatabaseError(format!("Failed to list cache objects: {}", e)))?;

        let mut objects = Vec::new();
        for row in rows {
            let key: String = row.get("key");
            let size: i64 = row.get("size");
            let content_type: Option<String> = row.get("content_type");
            let etag: Option<String> = row.get("etag");
            let metadata_json: serde_json::Value = row.get("metadata");
            let updated_at: DateTime<Utc> = row.get("updated_at");

            let metadata: HashMap<String, String> =
                serde_json::from_value(metadata_json).unwrap_or_default();

            objects.push(ObjectMetadata {
                key,
                size: size as u64,
                last_modified: updated_at,
                etag,
                content_type,
                metadata,
            });
        }

        Ok(objects)
    }

    async fn get(&self, key: &str) -> DomainResult<Vec<u8>> {
        let row = sqlx::query(
            r#"
            SELECT data FROM cache_objects WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to get cache object: {}", e)))?;

        match row {
            Some(row) => {
                let data: Vec<u8> = row.get("data");
                Ok(data)
            }
            None => Err(DomainError::NotFound(format!(
                "Cache object not found: {}",
                key
            ))),
        }
    }

    async fn put(
        &self, key: &str, data: &[u8], content_type: Option<&str>,
    ) -> DomainResult<ObjectMetadata> {
        let size = data.len() as i64;
        let etag = Self::generate_etag(data);
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO cache_objects (key, data, size, content_type, etag, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (key) DO UPDATE SET
                data = EXCLUDED.data,
                size = EXCLUDED.size,
                content_type = EXCLUDED.content_type,
                etag = EXCLUDED.etag,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(key)
        .bind(data)
        .bind(size)
        .bind(content_type)
        .bind(&etag)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to put cache object: {}", e)))?;

        Ok(ObjectMetadata {
            key: key.to_string(),
            size: size as u64,
            last_modified: now,
            etag: Some(etag),
            content_type: content_type.map(String::from),
            metadata: HashMap::new(),
        })
    }

    async fn delete(&self, key: &str) -> DomainResult<()> {
        sqlx::query(
            r#"
            DELETE FROM cache_objects WHERE key = $1
            "#,
        )
        .bind(key)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to delete cache object: {}", e)))?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> DomainResult<bool> {
        let row = sqlx::query(
            r#"
            SELECT 1 FROM cache_objects WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to check cache object: {}", e)))?;

        Ok(row.is_some())
    }

    async fn head(&self, key: &str) -> DomainResult<Option<ObjectMetadata>> {
        let row = sqlx::query(
            r#"
            SELECT key, size, content_type, etag, metadata, updated_at
            FROM cache_objects
            WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            DomainError::DatabaseError(format!("Failed to get cache object metadata: {}", e))
        })?;

        match row {
            Some(row) => {
                let key: String = row.get("key");
                let size: i64 = row.get("size");
                let content_type: Option<String> = row.get("content_type");
                let etag: Option<String> = row.get("etag");
                let metadata_json: serde_json::Value = row.get("metadata");
                let updated_at: DateTime<Utc> = row.get("updated_at");

                let metadata: HashMap<String, String> =
                    serde_json::from_value(metadata_json).unwrap_or_default();

                Ok(Some(ObjectMetadata {
                    key,
                    size: size as u64,
                    last_modified: updated_at,
                    etag,
                    content_type,
                    metadata,
                }))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {}
