use std::path::Path;
use std::sync::Arc;

use super::StorageManager;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::config::{
    PipedashConfig,
    StorageBackend as ConfigStorageBackend,
};
use crate::infrastructure::database::init_database;
#[cfg(feature = "postgres")]
use crate::infrastructure::secrets::PostgresTokenStore;
use crate::infrastructure::secrets::SqliteTokenStore;
use crate::infrastructure::storage::{
    LocalStorage,
    StorageBackend,
};
use crate::infrastructure::token_store::TokenStore;
#[cfg(feature = "postgres")]
use crate::infrastructure::PostgresConfigBackend;
use crate::infrastructure::{
    ConfigBackend,
    SqliteConfigBackend,
};

impl StorageManager {
    pub(super) async fn create_token_store(
        config: &PipedashConfig, backend: ConfigStorageBackend, is_desktop: bool, data_dir: &Path,
        vault_password: Option<String>,
    ) -> DomainResult<Arc<dyn TokenStore>> {
        match (backend, is_desktop) {
            (ConfigStorageBackend::Sqlite, true) if vault_password.is_none() => {
                Err(DomainError::InvalidConfig(
                    "Keyring backend must be created externally (Tauri-specific)".into(),
                ))
            }
            (ConfigStorageBackend::Sqlite, _) => {
                let db_path = data_dir.join("pipedash.db");
                let pool = init_database(db_path).await.map_err(|e| {
                    DomainError::DatabaseError(format!(
                        "Failed to initialize SQLite database for token storage: {}",
                        e
                    ))
                })?;

                let store = SqliteTokenStore::new(pool, vault_password).await?;
                Ok(Arc::new(store))
            }
            (ConfigStorageBackend::Postgres, _) => {
                #[cfg(feature = "postgres")]
                {
                    let connection_string = &config.storage.postgres.connection_string;
                    if connection_string.is_empty() {
                        return Err(DomainError::InvalidConfig(
                            "PostgreSQL connection string required for token storage".into(),
                        ));
                    }

                    let store = PostgresTokenStore::new(connection_string, vault_password).await?;
                    Ok(Arc::new(store))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = config; // silence unused warning
                    Err(DomainError::InvalidConfig(
                        "PostgreSQL feature not enabled".into(),
                    ))
                }
            }
        }
    }

    pub(super) async fn create_config_backend(
        config: &PipedashConfig, backend: ConfigStorageBackend, data_dir: &Path,
    ) -> DomainResult<(
        Arc<dyn ConfigBackend>,
        crate::infrastructure::database::DatabasePool,
    )> {
        match backend {
            ConfigStorageBackend::Sqlite => {
                let db_path = data_dir.join("pipedash.db");
                let pool = init_database(db_path).await.map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to initialize database: {}", e))
                })?;
                let db_pool = crate::infrastructure::database::DatabasePool::Sqlite(pool.clone());
                let config_backend = SqliteConfigBackend::new(pool);
                Ok((Arc::new(config_backend), db_pool))
            }
            ConfigStorageBackend::Postgres => {
                #[cfg(feature = "postgres")]
                {
                    if config.storage.postgres.connection_string.is_empty() {
                        return Err(DomainError::InvalidConfig(
                            "PostgreSQL connection string not configured".into(),
                        ));
                    }
                    let config_backend =
                        PostgresConfigBackend::new(&config.storage.postgres.connection_string)
                            .await?;
                    let db_pool = crate::infrastructure::database::DatabasePool::Postgres(
                        config_backend.pool().clone(),
                    );
                    Ok((Arc::new(config_backend), db_pool))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = config; // silence unused warning
                    Err(DomainError::InvalidConfig(
                        "PostgreSQL feature not enabled".into(),
                    ))
                }
            }
        }
    }

    pub(super) async fn create_cache_backend(
        config: &PipedashConfig, backend: ConfigStorageBackend, data_dir: &Path,
    ) -> DomainResult<Arc<dyn StorageBackend>> {
        match backend {
            ConfigStorageBackend::Sqlite => {
                let cache_dir = data_dir.join("cache");
                let storage = LocalStorage::new(cache_dir);
                storage.ensure_directories().await?;
                Ok(Arc::new(storage))
            }
            ConfigStorageBackend::Postgres => {
                #[cfg(feature = "postgres")]
                {
                    use crate::infrastructure::storage::PostgresStorage;

                    if config.storage.postgres.connection_string.is_empty() {
                        return Err(DomainError::InvalidConfig(
                            "PostgreSQL connection string not configured".into(),
                        ));
                    }
                    let storage = PostgresStorage::from_connection_string(
                        &config.storage.postgres.connection_string,
                    )
                    .await?;
                    Ok(Arc::new(storage))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = config; // silence unused warning
                    Err(DomainError::InvalidConfig(
                        "PostgreSQL feature not enabled".into(),
                    ))
                }
            }
        }
    }
}
