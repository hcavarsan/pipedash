use std::path::PathBuf;
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
use crate::infrastructure::storage::StorageBackend;
use crate::infrastructure::token_store::{
    MemoryTokenStore,
    TokenStore,
};
use crate::infrastructure::ConfigBackend;

mod backends;

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
        let data_dir = Self::prepare_data_dir(&config).await?;

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

        let vault_password = config
            .storage
            .vault_password
            .clone()
            .or_else(|| std::env::var("PIPEDASH_VAULT_PASSWORD").ok());
        let (token_store, vault_locked) =
            match Self::create_token_store(&config, backend, is_desktop, &data_dir, vault_password)
                .await
            {
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

        Self::assemble(config, token_store, vault_locked, is_desktop, data_dir).await
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
        use crate::infrastructure::database::DatabasePool;

        const QUERY: &str = "SELECT 1 FROM vault_verifier";
        let verifier = match &self.database_pool {
            DatabasePool::Sqlite(pool) => sqlx::query(QUERY)
                .fetch_optional(pool)
                .await
                .map(|r| r.is_some()),
            DatabasePool::Postgres(pool) => sqlx::query(QUERY)
                .fetch_optional(pool)
                .await
                .map(|r| r.is_some()),
        }
        .map_err(|e| DomainError::DatabaseError(format!("Failed to read vault verifier: {}", e)))?;

        Ok(!verifier && self.config_backend.list_providers().await?.is_empty())
    }

    pub async fn unlock_vault(&self, password: &str) -> DomainResult<()> {
        if !*self.vault_locked.read().await {
            return Ok(()); // Already unlocked
        }

        std::env::set_var("PIPEDASH_VAULT_PASSWORD", password);

        let new_store = Self::create_token_store(
            &self.config,
            self.config.storage.backend,
            self.is_desktop,
            &self.data_dir,
            Some(password.to_string()),
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
        Self::with_vault_state(config, token_store, is_desktop, false).await
    }

    pub async fn with_token_store_locked(
        config: PipedashConfig, token_store: Arc<dyn TokenStore>, is_desktop: bool,
    ) -> DomainResult<Self> {
        let manager = Self::with_vault_state(config, token_store, is_desktop, true).await?;
        tracing::info!("StorageManager created in locked vault mode");
        Ok(manager)
    }

    async fn with_vault_state(
        config: PipedashConfig, token_store: Arc<dyn TokenStore>, is_desktop: bool,
        vault_locked: bool,
    ) -> DomainResult<Self> {
        let data_dir = Self::prepare_data_dir(&config).await?;
        Self::assemble(config, token_store, vault_locked, is_desktop, data_dir).await
    }

    async fn prepare_data_dir(config: &PipedashConfig) -> DomainResult<PathBuf> {
        let data_dir = config.data_dir();

        tokio::fs::create_dir_all(&data_dir).await.map_err(|e| {
            DomainError::DatabaseError(format!("Failed to create data directory: {}", e))
        })?;

        Ok(data_dir)
    }

    async fn assemble(
        config: PipedashConfig, token_store: Arc<dyn TokenStore>, vault_locked: bool,
        is_desktop: bool, data_dir: PathBuf,
    ) -> DomainResult<Self> {
        let backend = config.storage.backend;
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
}

#[cfg(test)]
mod tests;
