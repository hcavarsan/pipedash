use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{
    broadcast,
    RwLock,
};

use super::loader::{
    ConfigLoader,
    Platform,
};
use super::schema::{
    ConfigKey,
    PipedashConfig,
    ProviderFileConfig,
};
use super::sync::{
    ProviderSyncService,
    SyncResult,
};
use super::token_ref::TokenReference;
use super::validation::ValidationResult;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::{
    ConfigBackend,
    TokenStore,
};

#[derive(Debug, Clone)]
pub enum ConfigChangeEvent {
    Reloaded {
        changed_keys: Vec<String>,
    },
    ValueChanged {
        key: String,
        old_value: Option<String>,
        new_value: String,
    },
    ProviderAdded {
        name: String,
    },
    ProviderUpdated {
        name: String,
    },
    ProviderRemoved {
        name: String,
    },
}

impl ConfigChangeEvent {
    pub fn summary(&self) -> String {
        match self {
            Self::Reloaded { changed_keys } => {
                format!("Config reloaded, {} keys changed", changed_keys.len())
            }
            Self::ValueChanged { key, .. } => format!("Value changed: {}", key),
            Self::ProviderAdded { name } => format!("Provider added: {}", name),
            Self::ProviderUpdated { name } => format!("Provider updated: {}", name),
            Self::ProviderRemoved { name } => format!("Provider removed: {}", name),
        }
    }
}

pub struct ConfigState {
    config: RwLock<Arc<PipedashConfig>>,

    config_path: PathBuf,

    change_tx: broadcast::Sender<ConfigChangeEvent>,

    token_store: Arc<dyn TokenStore>,

    platform: Platform,

    sync_service: ProviderSyncService,
}

impl ConfigState {
    pub async fn initialize(
        data_dir: &std::path::Path, token_store: Arc<dyn TokenStore>,
    ) -> DomainResult<Arc<Self>> {
        let platform = Platform::detect();

        let config_path = ConfigLoader::discover_config_path();
        let legacy_path = data_dir.join("storage_config.json");

        if legacy_path.exists() && !config_path.exists() {
            let migrated_config =
                super::migration::ConfigMigrator::migrate_if_needed(data_dir).await?;
            return Self::from_config_impl(migrated_config, config_path, token_store);
        }

        let config = ConfigLoader::load_or_create(&config_path, platform)
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to load config: {}", e)))?;

        let validation = config.validate();
        for error in &validation.errors {
            tracing::error!("Config error: {}", error);
        }
        for warning in &validation.warnings {
            tracing::warn!("Config warning: {}", warning);
        }

        let (change_tx, _) = broadcast::channel(16);

        let state = Arc::new(Self {
            config: RwLock::new(Arc::new(config)),
            config_path,
            change_tx,
            token_store,
            platform,
            sync_service: ProviderSyncService::new(),
        });

        Ok(state)
    }

    pub fn from_config(
        config: PipedashConfig, config_path: PathBuf, token_store: Arc<dyn TokenStore>,
    ) -> Arc<Self> {
        let (change_tx, _) = broadcast::channel(16);

        Arc::new(Self {
            config: RwLock::new(Arc::new(config)),
            config_path,
            change_tx,
            token_store,
            platform: Platform::detect(),
            sync_service: ProviderSyncService::new(),
        })
    }

    fn from_config_impl(
        config: PipedashConfig, config_path: PathBuf, token_store: Arc<dyn TokenStore>,
    ) -> DomainResult<Arc<Self>> {
        Ok(Self::from_config(config, config_path, token_store))
    }

    pub async fn get(&self) -> Arc<PipedashConfig> {
        self.config.read().await.clone()
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn token_store(&self) -> Arc<dyn TokenStore> {
        Arc::clone(&self.token_store)
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent> {
        self.change_tx.subscribe()
    }

    pub async fn resolve_token(
        &self, token_ref: &TokenReference, provider_id: Option<i64>,
    ) -> DomainResult<String> {
        token_ref
            .resolve(self.token_store.as_ref(), provider_id)
            .await
            .map_err(|e| DomainError::InvalidConfig(format!("Token resolution failed: {}", e)))
    }

    pub async fn reload(&self) -> DomainResult<ConfigChangeEvent> {
        let new_config = ConfigLoader::load(&self.config_path)
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to reload config: {}", e)))?;

        let validation = new_config.validate();
        if !validation.errors.is_empty() {
            return Err(DomainError::InvalidConfig(format!(
                "Config validation failed: {:?}",
                validation.errors
            )));
        }

        let event = ConfigChangeEvent::Reloaded {
            changed_keys: vec![], // TODO: Calculate diff
        };

        let _ = self.change_tx.send(event.clone());

        tracing::info!("Config reloaded from {:?}", self.config_path);
        Ok(event)
    }

    pub async fn validate(&self) -> ValidationResult {
        self.config.read().await.validate()
    }

    pub async fn add_provider(&self, id: String, provider: ProviderFileConfig) -> DomainResult<()> {
        if !provider.token.is_empty() {
            TokenReference::parse(&provider.token)
                .map_err(|e| DomainError::InvalidConfig(format!("Invalid token format: {}", e)))?;
        }

        let current_config = self.config.read().await;
        let mut config = (**current_config).clone();
        drop(current_config);

        if config.providers.contains_key(&id) {
            return Err(DomainError::InvalidConfig(format!(
                "Provider '{}' already exists",
                id
            )));
        }

        config.providers.insert(id.clone(), provider);

        self.persist_config(&config).await?;

        let event = ConfigChangeEvent::ProviderAdded { name: id };
        let _ = self.change_tx.send(event.clone());

        Ok(())
    }

    pub async fn update_provider(
        &self, id: &str, provider: ProviderFileConfig,
    ) -> DomainResult<()> {
        if !provider.token.is_empty() {
            TokenReference::parse(&provider.token)
                .map_err(|e| DomainError::InvalidConfig(format!("Invalid token format: {}", e)))?;
        }

        let current_config = self.config.read().await;
        let mut config = (**current_config).clone();
        drop(current_config);

        if !config.providers.contains_key(id) {
            return Err(DomainError::InvalidConfig(format!(
                "Provider '{}' not found",
                id
            )));
        }

        config.providers.insert(id.to_string(), provider);

        self.persist_config(&config).await?;

        let event = ConfigChangeEvent::ProviderUpdated {
            name: id.to_string(),
        };
        let _ = self.change_tx.send(event.clone());

        Ok(())
    }

    pub async fn remove_provider(&self, id: &str) -> DomainResult<()> {
        let current_config = self.config.read().await;
        let mut config = (**current_config).clone();
        drop(current_config);

        if config.providers.shift_remove(id).is_none() {
            return Err(DomainError::InvalidConfig(format!(
                "Provider '{}' not found",
                id
            )));
        }

        self.persist_config(&config).await?;

        let event = ConfigChangeEvent::ProviderRemoved {
            name: id.to_string(),
        };
        let _ = self.change_tx.send(event.clone());

        Ok(())
    }

    pub async fn update_general_settings(
        &self, metrics_enabled: bool, default_refresh_interval: u32,
    ) -> DomainResult<()> {
        let current_config = self.config.read().await;
        let mut config = (**current_config).clone();
        drop(current_config);

        config.general.metrics_enabled = metrics_enabled;
        config.general.default_refresh_interval = default_refresh_interval;

        self.persist_config(&config).await?;

        let _ = self.change_tx.send(ConfigChangeEvent::ValueChanged {
            key: "general".to_string(),
            old_value: None,
            new_value: "updated".to_string(),
        });

        Ok(())
    }

    pub async fn sync_providers_from_toml(
        &self, config_backend: &dyn ConfigBackend, delete_orphans: bool,
    ) -> DomainResult<SyncResult> {
        let config = self.config.read().await;
        self.sync_service
            .sync_toml_to_db(
                config.providers.clone(),
                config_backend,
                self.token_store.as_ref(),
                delete_orphans,
            )
            .await
    }

    async fn persist_config(&self, config: &PipedashConfig) -> DomainResult<()> {
        let existing_content = std::fs::read_to_string(&self.config_path).ok();

        let new_content = if let Some(existing) = existing_content {
            Self::update_toml_preserving_comments(&existing, config)?
        } else {
            ConfigLoader::to_toml(config).map_err(|e| {
                DomainError::DatabaseError(format!("Failed to serialize config: {}", e))
            })?
        };

        std::fs::write(&self.config_path, &new_content).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to write config file: {}", e))
        })?;

        tracing::debug!("Config persisted to {:?}", self.config_path);
        Ok(())
    }

    fn update_toml_preserving_comments(
        existing: &str, config: &PipedashConfig,
    ) -> DomainResult<String> {
        use toml_edit::{
            value,
            DocumentMut,
            Item,
        };

        let mut doc: DocumentMut = existing.parse().map_err(|e| {
            DomainError::DatabaseError(format!("Failed to parse existing TOML: {}", e))
        })?;

        if let Some(general) = doc.get_mut("general") {
            if let Some(table) = general.as_table_mut() {
                table["metrics_enabled"] = value(config.general.metrics_enabled);
                table["default_refresh_interval"] =
                    value(config.general.default_refresh_interval as i64);
            }
        }

        if let Some(server) = doc.get_mut("server") {
            if let Some(table) = server.as_table_mut() {
                table["bind_addr"] = value(&config.server.bind_addr);
                table["cors_allow_all"] = value(config.server.cors_allow_all);
            }
        }

        doc.remove("providers");

        if !config.providers.is_empty() {
            let mut providers_table = toml_edit::Table::new();

            for (id, provider) in &config.providers {
                let mut provider_table = toml_edit::Table::new();

                if let Some(name) = &provider.name {
                    provider_table["name"] = value(name);
                }

                provider_table["type"] = value(&provider.provider_type);
                provider_table["token"] = value(&provider.token);
                provider_table["refresh_interval"] = value(provider.refresh_interval as i64);

                if !provider.config.is_empty() {
                    let mut config_table = toml_edit::Table::new();
                    for (k, v) in &provider.config {
                        config_table[k] = value(v);
                    }
                    provider_table["config"] = Item::Table(config_table);
                }

                providers_table[id] = Item::Table(provider_table);
            }

            doc["providers"] = Item::Table(providers_table);
        }

        Ok(doc.to_string())
    }

    pub async fn get_string(&self, key: ConfigKey) -> String {
        if let Ok(val) = std::env::var(key.env_var_name()) {
            return val;
        }

        let config = self.config.read().await;
        match key {
            ConfigKey::MetricsEnabled => config.general.metrics_enabled.to_string(),
            ConfigKey::DefaultRefreshInterval => {
                config.general.default_refresh_interval.to_string()
            }
            ConfigKey::BindAddr => config.server.bind_addr.clone(),
            ConfigKey::CorsAllowAll => config.server.cors_allow_all.to_string(),
            ConfigKey::DataDir => {
                if config.storage.data_dir.is_empty() {
                    PipedashConfig::default_data_dir()
                        .to_string_lossy()
                        .to_string()
                } else {
                    config.storage.data_dir.clone()
                }
            }
            ConfigKey::StorageBackend => config.storage.backend.to_string(),
            ConfigKey::PostgresConnectionString => {
                config.storage.postgres.connection_string.clone()
            }
        }
    }

    pub async fn get_bool(&self, key: ConfigKey) -> bool {
        let val = self.get_string(key).await;
        matches!(val.to_lowercase().as_str(), "true" | "1" | "yes")
    }

    pub async fn get_u32(&self, key: ConfigKey) -> u32 {
        self.get_string(key).await.parse().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::infrastructure::MemoryTokenStore;

    fn create_test_state(temp_dir: &TempDir, token_store: Arc<dyn TokenStore>) -> Arc<ConfigState> {
        let config_path = temp_dir.path().join("config.toml");
        let config = PipedashConfig::default();
        ConfigState::from_config(config, config_path, token_store)
    }

    #[tokio::test]
    async fn test_config_state_from_config() {
        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        let config = state.get().await;
        assert!(config.general.metrics_enabled);
    }

    #[tokio::test]
    async fn test_add_remove_provider() {
        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        let provider = ProviderFileConfig {
            name: Some("Test GitHub".to_string()),
            provider_type: "github".to_string(),
            token: "${GITHUB_TOKEN:-}".to_string(),
            refresh_interval: 30,
            config: std::collections::HashMap::new(),
        };

        state
            .add_provider("test-github".to_string(), provider)
            .await
            .unwrap();

        let config = state.get().await;
        assert_eq!(config.providers.len(), 1);
        assert!(config.providers.contains_key("test-github"));

        state.remove_provider("test-github").await.unwrap();

        let config = state.get().await;
        assert!(config.providers.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_to_changes() {
        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        let mut rx = state.subscribe();

        let provider = ProviderFileConfig {
            name: None,
            provider_type: "github".to_string(),
            token: String::new(),
            refresh_interval: 30,
            config: std::collections::HashMap::new(),
        };

        state
            .add_provider("test".to_string(), provider)
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, ConfigChangeEvent::ProviderAdded { .. }));
    }

    #[tokio::test]
    async fn test_persist_and_reload() {
        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());
        let config_path = temp_dir.path().join("config.toml");

        let state = ConfigState::from_config(
            PipedashConfig::default(),
            config_path.clone(),
            token_store.clone(),
        );

        let provider = ProviderFileConfig {
            name: Some("Persist Test".to_string()),
            provider_type: "gitlab".to_string(),
            token: "${GITLAB_TOKEN:-}".to_string(),
            refresh_interval: 60,
            config: std::collections::HashMap::new(),
        };

        state
            .add_provider("persist-test".to_string(), provider)
            .await
            .unwrap();

        let loaded_config = ConfigLoader::load(&config_path).unwrap();
        assert_eq!(loaded_config.providers.len(), 1);
        assert!(loaded_config.providers.contains_key("persist-test"));
        assert_eq!(loaded_config.providers["persist-test"].refresh_interval, 60);
    }

    #[tokio::test]
    async fn test_get_config_values() {
        let old_metrics = std::env::var("PIPEDASH_METRICS_ENABLED").ok();
        let old_refresh = std::env::var("PIPEDASH_DEFAULT_REFRESH_INTERVAL").ok();
        std::env::remove_var("PIPEDASH_METRICS_ENABLED");
        std::env::remove_var("PIPEDASH_DEFAULT_REFRESH_INTERVAL");

        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        assert!(state.get_bool(ConfigKey::MetricsEnabled).await);
        assert_eq!(state.get_u32(ConfigKey::DefaultRefreshInterval).await, 30);

        if let Some(v) = old_metrics {
            std::env::set_var("PIPEDASH_METRICS_ENABLED", v);
        }
        if let Some(v) = old_refresh {
            std::env::set_var("PIPEDASH_DEFAULT_REFRESH_INTERVAL", v);
        }
    }

    #[tokio::test]
    async fn test_env_var_override() {
        let old_value = std::env::var("PIPEDASH_METRICS_ENABLED").ok();
        std::env::set_var("PIPEDASH_METRICS_ENABLED", "false");

        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        assert!(!state.get_bool(ConfigKey::MetricsEnabled).await);

        match old_value {
            Some(v) => std::env::set_var("PIPEDASH_METRICS_ENABLED", v),
            None => std::env::remove_var("PIPEDASH_METRICS_ENABLED"),
        }
    }
}
