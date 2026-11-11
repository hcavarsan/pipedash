#[cfg(feature = "postgres")]
pub mod postgres;

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{
    DateTime,
    Utc,
};
#[cfg(feature = "postgres")]
pub use postgres::PostgresStorage;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::fs;

use crate::domain::{
    DomainError,
    DomainResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub key: String,
    pub size: u64,
    pub last_modified: DateTime<Utc>,
    pub etag: Option<String>,
    pub content_type: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn is_available(&self) -> bool;

    async fn list(&self, prefix: Option<&str>) -> DomainResult<Vec<ObjectMetadata>>;

    async fn get(&self, key: &str) -> DomainResult<Vec<u8>>;

    async fn put(
        &self, key: &str, data: &[u8], content_type: Option<&str>,
    ) -> DomainResult<ObjectMetadata>;
}

pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn full_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }

    pub fn config_dir(&self) -> PathBuf {
        self.base_path.join("config")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.base_path.join("cache")
    }

    pub async fn ensure_directories(&self) -> DomainResult<()> {
        fs::create_dir_all(&self.base_path)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to create data dir: {}", e)))?;

        fs::create_dir_all(self.config_dir()).await.map_err(|e| {
            DomainError::DatabaseError(format!("Failed to create config dir: {}", e))
        })?;

        fs::create_dir_all(self.cache_dir()).await.map_err(|e| {
            DomainError::DatabaseError(format!("Failed to create cache dir: {}", e))
        })?;

        Ok(())
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn is_available(&self) -> bool {
        fs::metadata(&self.base_path).await.is_ok()
    }

    async fn list(&self, prefix: Option<&str>) -> DomainResult<Vec<ObjectMetadata>> {
        let search_path = match prefix {
            Some(p) => self.full_path(p),
            None => self.base_path.clone(),
        };

        if !search_path.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&search_path)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to read directory: {}", e)))?;

        let mut results = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to read entry: {}", e)))?
        {
            let metadata = entry.metadata().await.map_err(|e| {
                DomainError::DatabaseError(format!("Failed to read metadata: {}", e))
            })?;

            if metadata.is_file() {
                let key = entry
                    .path()
                    .strip_prefix(&self.base_path)
                    .unwrap_or(entry.path().as_path())
                    .to_string_lossy()
                    .to_string();

                let last_modified = metadata
                    .modified()
                    .map(DateTime::<Utc>::from)
                    .unwrap_or_else(|_| Utc::now());

                results.push(ObjectMetadata {
                    key,
                    size: metadata.len(),
                    last_modified,
                    etag: None,
                    content_type: None,
                    metadata: HashMap::new(),
                });
            }
        }

        Ok(results)
    }

    async fn get(&self, key: &str) -> DomainResult<Vec<u8>> {
        fs::read(self.full_path(key))
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to read file: {}", e)))
    }

    async fn put(
        &self, key: &str, data: &[u8], _content_type: Option<&str>,
    ) -> DomainResult<ObjectMetadata> {
        let path = self.full_path(key);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| DomainError::DatabaseError(format!("Failed to create dir: {}", e)))?;
        }

        fs::write(&path, data)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to write file: {}", e)))?;

        let metadata = fs::metadata(&path)
            .await
            .map_err(|e| DomainError::DatabaseError(format!("Failed to read metadata: {}", e)))?;

        let last_modified = metadata
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());

        Ok(ObjectMetadata {
            key: key.to_string(),
            size: metadata.len(),
            last_modified,
            etag: None,
            content_type: None,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests;
