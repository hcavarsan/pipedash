use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use tokio::sync::RwLock;

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
use crate::infrastructure::token_store::{
    MemoryTokenStore,
    TokenStore,
};
#[cfg(feature = "postgres")]
use crate::infrastructure::PostgresConfigBackend;
use crate::infrastructure::{
    ConfigBackend,
    SqliteConfigBackend,
};

pub struct StorageManager {
    config: PipedashConfig,
    token_store: Arc<RwLock<Arc<dyn TokenStore>>>,
    config_backend: Arc<dyn ConfigBackend>,
    cache_backend: Arc<dyn StorageBackend>,
    database_pool: crate::infrastructure::database::DatabasePool,
    vault_locked: Arc<RwLock<bool>>,
    data_dir: PathBuf,
    is_desktop: bool,
}

impl StorageManager {
    pub async fn from_config(config: PipedashConfig, is_desktop: bool) -> DomainResult<Self> {
        Self::from_config_internal(config, is_desktop, false).await
    }

    pub async fn from_config_allow_locked(
        config: PipedashConfig, is_desktop: bool,
    ) -> DomainResult<Self> {
        Self::from_config_internal(config, is_desktop, true).await
    }

    async fn from_config_internal(
        config: PipedashConfig, is_desktop: bool, allow_locked: bool,
    ) -> DomainResult<Self> {
        let data_dir = config.data_dir();

        std::fs::create_dir_all(&data_dir).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to create data directory: {}", e))
        })?;

        let backend = config.storage.backend;

        #[cfg(feature = "postgres")]
        if backend == ConfigStorageBackend::Postgres {
            use crate::infrastructure::database::init_postgres_database;

            init_postgres_database(&config.storage.postgres.connection_string)
                .await
                .map_err(|e| {
                    DomainError::DatabaseError(format!(
                        "Failed to initialize PostgreSQL database: {}",
                        e
                    ))
                })?;

            tracing::info!("PostgreSQL migrations completed");
        }

        let (token_store, vault_locked) =
            match Self::create_token_store(&config, backend, is_desktop, &data_dir).await {
                Ok(store) => (store, false),
                Err(e) if allow_locked && Self::is_vault_password_error(&e) => {
                    tracing::warn!("Vault password not set - starting in locked mode");
                    (
                        Arc::new(MemoryTokenStore::new()) as Arc<dyn TokenStore>,
                        true,
                    )
                }
                Err(e) => return Err(e),
            };

        let (config_backend, database_pool) =
            Self::create_config_backend(&config, backend, &data_dir).await?;
        let cache_backend = Self::create_cache_backend(&config, backend, &data_dir).await?;

        Ok(Self {
            config,
            token_store: Arc::new(RwLock::new(token_store)),
            config_backend,
            cache_backend,
            database_pool,
            vault_locked: Arc::new(RwLock::new(vault_locked)),
            data_dir,
            is_desktop,
        })
    }

    fn is_vault_password_error(error: &DomainError) -> bool {
        match error {
            DomainError::InvalidConfig(msg) => {
                msg.contains("PIPEDASH_VAULT_PASSWORD") || msg.contains("vault password")
            }
            DomainError::AuthenticationFailed(msg) => {
                msg.contains("vault password") || msg.contains("decrypt")
            }
            _ => false,
        }
    }

    pub async fn is_vault_locked(&self) -> bool {
        *self.vault_locked.read().await
    }

    pub fn requires_vault_password(&self) -> bool {
        match (self.config.storage.backend, self.is_desktop) {
            (ConfigStorageBackend::Sqlite, true) => false, // Keyring
            (ConfigStorageBackend::Sqlite, false) => true, // SQLite encrypted
            (ConfigStorageBackend::Postgres, _) => true,   // Postgres encrypted
        }
    }

    pub async fn is_first_time_vault_setup(&self) -> DomainResult<bool> {
        let providers = self.config_backend.list_providers().await?;
        Ok(providers.is_empty())
    }

    pub async fn unlock_vault(&self, password: &str) -> DomainResult<()> {
        if !*self.vault_locked.read().await {
            return Ok(()); // Already unlocked
        }

        std::env::set_var("PIPEDASH_VAULT_PASSWORD", password);

        let new_store = Self::create_token_store_with_password(
            &self.config,
            self.config.storage.backend,
            self.is_desktop,
            &self.data_dir,
            password,
        )
        .await?;

        {
            let mut store = self.token_store.write().await;
            *store = new_store;
        }

        {
            let mut locked = self.vault_locked.write().await;
            *locked = false;
        }

        tracing::info!("Vault unlocked successfully");
        Ok(())
    }

    async fn create_token_store_with_password(
        config: &PipedashConfig, backend: ConfigStorageBackend, _is_desktop: bool, data_dir: &Path,
        password: &str,
    ) -> DomainResult<Arc<dyn TokenStore>> {
        match backend {
            ConfigStorageBackend::Sqlite => {
                let db_path = data_dir.join("pipedash.db");
                let pool = init_database(db_path).await.map_err(|e| {
                    DomainError::DatabaseError(format!(
                        "Failed to initialize SQLite database for token storage: {}",
                        e
                    ))
                })?;

                let store = SqliteTokenStore::new(pool, Some(password.to_string())).await?;
                Ok(Arc::new(store))
            }
            ConfigStorageBackend::Postgres => {
                #[cfg(feature = "postgres")]
                {
                    let connection_string = &config.storage.postgres.connection_string;
                    if connection_string.is_empty() {
                        return Err(DomainError::InvalidConfig(
                            "PostgreSQL connection string required for token storage".into(),
                        ));
                    }

                    let store =
                        PostgresTokenStore::new(connection_string, Some(password.to_string()))
                            .await?;
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

    async fn create_token_store(
        config: &PipedashConfig, backend: ConfigStorageBackend, is_desktop: bool, data_dir: &Path,
    ) -> DomainResult<Arc<dyn TokenStore>> {
        let vault_password = config
            .storage
            .vault_password
            .clone()
            .or_else(|| std::env::var("PIPEDASH_VAULT_PASSWORD").ok());

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

                    let vault_password = config
                        .storage
                        .vault_password
                        .clone()
                        .or_else(|| std::env::var("PIPEDASH_VAULT_PASSWORD").ok());
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

    async fn create_config_backend(
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

    async fn create_cache_backend(
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

    pub fn config(&self) -> &PipedashConfig {
        &self.config
    }

    pub async fn token_store(&self) -> Arc<dyn TokenStore> {
        let guard = self.token_store.read().await;
        Arc::clone(&guard)
    }

    pub fn config_backend(&self) -> Arc<dyn ConfigBackend> {
        Arc::clone(&self.config_backend)
    }

    pub fn cache_backend(&self) -> Arc<dyn StorageBackend> {
        Arc::clone(&self.cache_backend)
    }

    pub fn database_pool(&self) -> crate::infrastructure::database::DatabasePool {
        self.database_pool.clone()
    }

    pub async fn with_token_store(
        config: PipedashConfig, token_store: Arc<dyn TokenStore>, is_desktop: bool,
    ) -> DomainResult<Self> {
        let data_dir = config.data_dir();
        let backend = config.storage.backend;

        std::fs::create_dir_all(&data_dir).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to create data directory: {}", e))
        })?;

        let (config_backend, database_pool) =
            Self::create_config_backend(&config, backend, &data_dir).await?;
        let cache_backend = Self::create_cache_backend(&config, backend, &data_dir).await?;

        Ok(Self {
            config,
            token_store: Arc::new(RwLock::new(token_store)),
            config_backend,
            cache_backend,
            database_pool,
            vault_locked: Arc::new(RwLock::new(false)), // Custom token store means not locked
            data_dir,
            is_desktop,
        })
    }

    pub async fn with_token_store_locked(
        config: PipedashConfig, token_store: Arc<dyn TokenStore>, is_desktop: bool,
    ) -> DomainResult<Self> {
        let data_dir = config.data_dir();
        let backend = config.storage.backend;

        std::fs::create_dir_all(&data_dir).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to create data directory: {}", e))
        })?;

        let (config_backend, database_pool) =
            Self::create_config_backend(&config, backend, &data_dir).await?;
        let cache_backend = Self::create_cache_backend(&config, backend, &data_dir).await?;

        tracing::info!("StorageManager created in locked vault mode");

        Ok(Self {
            config,
            token_store: Arc::new(RwLock::new(token_store)),
            config_backend,
            cache_backend,
            database_pool,
            vault_locked: Arc::new(RwLock::new(true)), // Vault is locked
            data_dir,
            is_desktop,
        })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_storage_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = PipedashConfig::default();
        config.storage.data_dir = temp_dir.path().to_string_lossy().to_string();

        let token_store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new());

        let manager = StorageManager::with_token_store(config, token_store, true)
            .await
            .unwrap();

        assert!(manager.token_store().await.get_token(1).await.is_err());
        assert!(manager.cache_backend().is_available().await);
    }

    #[tokio::test]
    async fn test_storage_manager_vault_locked_mode() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = PipedashConfig::default();
        config.storage.data_dir = temp_dir.path().to_string_lossy().to_string();

        let manager = StorageManager::from_config_allow_locked(config, false)
            .await
            .unwrap();

        assert!(manager.is_vault_locked().await);
        assert!(manager.requires_vault_password());
    }

    #[test]
    fn test_storage_config_defaults() {
        let config = PipedashConfig::default();

        assert_eq!(config.storage.backend, ConfigStorageBackend::Sqlite);
    }

    #[test]
    fn test_requires_vault_password() {
        let config = PipedashConfig::default();

        let is_desktop = true;
        let is_server = false;

        match (config.storage.backend, is_desktop) {
            (ConfigStorageBackend::Sqlite, true) => {} // Keyring - expected
            _ => panic!("Expected Keyring for desktop SQLite"),
        }

        match (config.storage.backend, is_server) {
            (ConfigStorageBackend::Sqlite, false) => {} // SQLite encrypted - expected
            _ => panic!("Expected SQLite encrypted for server SQLite"),
        }
    }
}
