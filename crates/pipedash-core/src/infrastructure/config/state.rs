use std::path::PathBuf;
use std::sync::{
    Arc,
    OnceLock,
};

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
    GeneralSettingsUpdate,
    PipedashConfig,
    ProviderFileConfig,
    MAX_REFRESH_INTERVAL_SECS,
    MIN_REFRESH_INTERVAL_SECS,
};
use super::sync::{
    ProviderSyncService,
    SyncResult,
};
use super::token_ref::TokenReference;
use super::validation::ValidationResult;
use crate::application::{
    HealthHandle,
    HealthInput,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::http_client::parse_proxy_url;
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

    health: OnceLock<HealthHandle>,
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
            health: OnceLock::new(),
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
            health: OnceLock::new(),
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

    pub fn attach_health(&self, health: HealthHandle) {
        let _ = self.health.set(health);
    }

    fn notify_settings_changed(&self) {
        if let Some(health) = self.health.get() {
            health.send(HealthInput::SettingsChanged);
        }
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

        *self.config.write().await = Arc::new(new_config);

        let event = ConfigChangeEvent::Reloaded {
            changed_keys: vec![], // TODO: Calculate diff
        };

        let _ = self.change_tx.send(event.clone());
        self.notify_settings_changed();

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
        let content = match std::fs::read_to_string(&self.config_path) {
            Ok(content) => Some(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(DomainError::DatabaseError(format!(
                    "Failed to read config file: {e}"
                )))
            }
        };
        if let Some(content) = content {
            let mut doc: toml_edit::DocumentMut = content.parse().map_err(|e| {
                DomainError::DatabaseError(format!("Failed to parse existing TOML: {e}"))
            })?;
            if doc
                .get_mut("providers")
                .and_then(toml_edit::Item::as_table_like_mut)
                .and_then(|providers| providers.remove(id))
                .is_some()
            {
                std::fs::write(&self.config_path, doc.to_string()).map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to write config file: {e}"))
                })?;
            }
        }

        let mut current = self.config.write().await;
        if current.providers.contains_key(id) {
            let mut config = (**current).clone();
            config.providers.shift_remove(id);
            *current = Arc::new(config);
        }

        let event = ConfigChangeEvent::ProviderRemoved {
            name: id.to_string(),
        };
        let _ = self.change_tx.send(event);
        Ok(())
    }

    pub async fn update_general_settings(&self, update: GeneralSettingsUpdate) -> DomainResult<()> {
        if !(MIN_REFRESH_INTERVAL_SECS..=MAX_REFRESH_INTERVAL_SECS)
            .contains(&update.default_refresh_interval)
        {
            return Err(DomainError::InvalidConfig(format!(
                "default_refresh_interval must be between {MIN_REFRESH_INTERVAL_SECS} and \
                 {MAX_REFRESH_INTERVAL_SECS} seconds"
            )));
        }
        let proxy_url = parse_proxy_url(&update.proxy_url)?
            .map(|_| update.proxy_url.trim().to_string())
            .unwrap_or_default();

        let current_config = self.config.read().await;
        let mut config = (**current_config).clone();
        drop(current_config);

        config.general.metrics_enabled = update.metrics_enabled;
        config.general.default_refresh_interval = update.default_refresh_interval;
        config.general.status_pages_enabled = update.status_pages_enabled;
        config.general.proxy_url = proxy_url;

        self.persist_general(&config).await?;

        let _ = self.change_tx.send(ConfigChangeEvent::ValueChanged {
            key: "general".to_string(),
            old_value: None,
            new_value: "updated".to_string(),
        });
        self.notify_settings_changed();

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

    pub fn load_providers(&self) -> DomainResult<indexmap::IndexMap<String, ProviderFileConfig>> {
        let file = ConfigLoader::load(&self.config_path)
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to reload config: {}", e)))?;

        let validation = file.validate();
        if !validation.errors.is_empty() {
            return Err(DomainError::InvalidConfig(format!(
                "Config validation failed: {:?}",
                validation.errors
            )));
        }

        Ok(file.providers)
    }

    pub async fn remember_provider(&self, id: &str, provider: &ProviderFileConfig) {
        let mut config = self.config.write().await;
        let mut next = (**config).clone();
        next.providers.insert(id.to_string(), provider.clone());
        *config = Arc::new(next);
    }

    pub async fn remember_providers(
        &self, providers: indexmap::IndexMap<String, ProviderFileConfig>,
    ) {
        let mut config = self.config.write().await;
        let mut next = (**config).clone();
        next.providers = providers;
        *config = Arc::new(next);
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

        *self.config.write().await = Arc::new(config.clone());

        tracing::debug!("Config persisted to {:?}", self.config_path);
        Ok(())
    }

    async fn persist_general(&self, config: &PipedashConfig) -> DomainResult<()> {
        let content = match std::fs::read_to_string(&self.config_path) {
            Ok(existing) => {
                let mut doc: toml_edit::DocumentMut = existing.parse().map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to parse existing TOML: {}", e))
                })?;
                ConfigLoader::apply_general(&mut doc, &config.general);
                doc.to_string()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ConfigLoader::to_toml(config)
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to serialize config: {}", e))
                })?,
            Err(e) => {
                return Err(DomainError::DatabaseError(format!(
                    "Failed to read config file: {}",
                    e
                )))
            }
        };

        std::fs::write(&self.config_path, content).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to write config file: {}", e))
        })?;

        *self.config.write().await = Arc::new(config.clone());

        tracing::debug!("General settings persisted to {:?}", self.config_path);
        Ok(())
    }

    pub(super) fn update_toml_preserving_comments(
        existing: &str, config: &PipedashConfig,
    ) -> DomainResult<String> {
        use toml_edit::{
            value,
            DocumentMut,
        };

        let mut doc: DocumentMut = existing.parse().map_err(|e| {
            DomainError::DatabaseError(format!("Failed to parse existing TOML: {}", e))
        })?;

        ConfigLoader::apply_general(&mut doc, &config.general);

        if let Some(server) = doc.get_mut("server") {
            if let Some(table) = server.as_table_mut() {
                table["bind_addr"] = value(&config.server.bind_addr);
                table["cors_allow_all"] = value(config.server.cors_allow_all);
            }
        }

        Self::write_providers(&mut doc, &config.providers);

        Ok(doc.to_string())
    }

    pub(super) fn write_providers(
        doc: &mut toml_edit::DocumentMut,
        providers: &indexmap::IndexMap<String, ProviderFileConfig>,
    ) {
        use toml_edit::{
            value,
            Item,
        };

        doc.remove("providers");

        if providers.is_empty() {
            return;
        }

        let mut providers_table = toml_edit::Table::new();

        for (id, provider) in providers {
            let mut provider_table = toml_edit::Table::new();

            if let Some(name) = &provider.name {
                provider_table["name"] = value(name);
            }

            provider_table["type"] = value(&provider.provider_type);
            provider_table["token"] = value(&provider.token);
            provider_table["refresh_interval"] = value(provider.refresh_interval as i64);

            if !provider.config.is_empty() {
                let mut config_table = toml_edit::Table::new();
                let mut entries: Vec<_> = provider.config.iter().collect();
                entries.sort();
                for (k, v) in entries {
                    config_table[k] = value(v);
                }
                provider_table["config"] = Item::Table(config_table);
            }

            providers_table[id] = Item::Table(provider_table);
        }

        doc["providers"] = Item::Table(providers_table);
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
            ConfigKey::StatusPagesEnabled => config.general.status_pages_enabled.to_string(),
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
            ConfigKey::PushRelayUrl => config.push.relay_url.clone(),
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
    use crate::{
        infrastructure::MemoryTokenStore,
        test_env::EnvGuard,
    };

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
    async fn test_reload_replaces_in_memory_config() {
        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        let mut on_disk = PipedashConfig::default();
        on_disk.general.default_refresh_interval = 123;
        std::fs::write(
            temp_dir.path().join("config.toml"),
            ConfigLoader::to_toml(&on_disk).unwrap(),
        )
        .unwrap();

        state.reload().await.unwrap();

        assert_eq!(state.get().await.general.default_refresh_interval, 123);
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
        let _env = EnvGuard::set(&[
            ("PIPEDASH_METRICS_ENABLED", None),
            ("PIPEDASH_DEFAULT_REFRESH_INTERVAL", None),
        ]);

        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        assert!(state.get_bool(ConfigKey::MetricsEnabled).await);
        assert_eq!(state.get_u32(ConfigKey::DefaultRefreshInterval).await, 30);
    }

    #[tokio::test]
    async fn test_env_var_override() {
        let _env = EnvGuard::set(&[("PIPEDASH_METRICS_ENABLED", Some("false"))]);

        let temp_dir = TempDir::new().unwrap();
        let token_store = Arc::new(MemoryTokenStore::new());

        let state = create_test_state(&temp_dir, token_store);

        assert!(!state.get_bool(ConfigKey::MetricsEnabled).await);
    }

    #[tokio::test]
    async fn test_push_relay_url_comes_from_the_file_unless_the_env_var_overrides_it() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = PipedashConfig::default();
        config.push.relay_url = "https://file.example.com".to_string();
        let state = ConfigState::from_config(
            config,
            temp_dir.path().join("config.toml"),
            Arc::new(MemoryTokenStore::new()),
        );

        {
            let _env = EnvGuard::set(&[("PIPEDASH_PUSH_RELAY_URL", None)]);
            assert_eq!(
                state.get_string(ConfigKey::PushRelayUrl).await,
                "https://file.example.com"
            );
        }

        let _env = EnvGuard::set(&[("PIPEDASH_PUSH_RELAY_URL", Some("https://env.example.com"))]);
        assert_eq!(
            state.get_string(ConfigKey::PushRelayUrl).await,
            "https://env.example.com"
        );
    }

    fn update(default_refresh_interval: u32, status_pages_enabled: bool) -> GeneralSettingsUpdate {
        GeneralSettingsUpdate {
            metrics_enabled: true,
            default_refresh_interval,
            status_pages_enabled,
            proxy_url: String::new(),
        }
    }

    #[tokio::test]
    async fn test_update_general_settings_saves_a_proxy_and_refuses_an_invalid_one() {
        let temp_dir = TempDir::new().unwrap();
        let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
        let path = temp_dir.path().join("config.toml");
        let with_proxy = |proxy_url: &str| GeneralSettingsUpdate {
            proxy_url: proxy_url.to_string(),
            ..update(30, true)
        };

        state
            .update_general_settings(with_proxy(" http://proxy.corp:3128 "))
            .await
            .unwrap();
        let refused = state
            .update_general_settings(with_proxy("http://alice:secret@proxy.corp:3128"))
            .await;

        assert!(matches!(refused, Err(DomainError::InvalidConfig(_))));
        assert_eq!(
            ConfigLoader::load(&path).unwrap().general.proxy_url,
            "http://proxy.corp:3128"
        );
        assert_eq!(
            state.get().await.general.proxy_url,
            "http://proxy.corp:3128"
        );

        state.update_general_settings(with_proxy("")).await.unwrap();

        assert_eq!(ConfigLoader::load(&path).unwrap().general.proxy_url, "");
    }

    #[tokio::test]
    async fn test_update_general_settings_creates_the_general_table_and_survives_a_reload() {
        let _env = EnvGuard::set(&[("PIPEDASH_STATUS_PAGES_ENABLED", None)]);
        let temp_dir = TempDir::new().unwrap();
        let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
        let path = temp_dir.path().join("config.toml");
        std::fs::write(&path, "# keep me\n[storage]\nbackend = \"sqlite\"\n").unwrap();
        let (health, mut receivers) = HealthHandle::channel();
        state.attach_health(health);

        state
            .update_general_settings(update(60, false))
            .await
            .unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        state.reload().await.unwrap();

        assert!(saved.contains("# keep me"));
        assert!(saved.contains("[general]"));
        assert!(
            !ConfigLoader::load(&path)
                .unwrap()
                .general
                .status_pages_enabled
        );
        let config = state.get().await;
        assert!(!config.general.status_pages_enabled);
        assert_eq!(config.general.default_refresh_interval, 60);
        assert!(!state.get_bool(ConfigKey::StatusPagesEnabled).await);
        assert!(matches!(
            receivers.inputs.try_recv(),
            Ok(HealthInput::SettingsChanged)
        ));
        assert!(matches!(
            receivers.inputs.try_recv(),
            Ok(HealthInput::SettingsChanged)
        ));
    }

    #[tokio::test]
    async fn test_update_general_settings_rejects_intervals_outside_five_to_three_hundred() {
        let temp_dir = TempDir::new().unwrap();
        let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
        let (health, mut receivers) = HealthHandle::channel();
        state.attach_health(health);

        for interval in [0, 4, 301, 100_000] {
            let result = state.update_general_settings(update(interval, false)).await;

            assert!(
                matches!(result, Err(DomainError::InvalidConfig(_))),
                "{interval}"
            );
        }
        assert!(state.get().await.general.status_pages_enabled);
        assert!(!temp_dir.path().join("config.toml").exists());
        assert!(receivers.inputs.try_recv().is_err());

        for interval in [5, 300] {
            state
                .update_general_settings(update(interval, true))
                .await
                .unwrap();
            assert_eq!(state.get().await.general.default_refresh_interval, interval);
            assert!(matches!(
                receivers.inputs.try_recv(),
                Ok(HealthInput::SettingsChanged)
            ));
        }
    }

    #[tokio::test]
    async fn test_update_general_settings_leaves_providers_and_secrets_alone() {
        let _env = EnvGuard::set(&[("PIPEDASH_TEST_TOKEN", Some("secret-value"))]);
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        let original = "[general]\nmetrics_enabled = true\n\n[providers.main]\ntype = \"github\"\ntoken = \"${PIPEDASH_TEST_TOKEN}\"\nrefresh_interval = 45\n";
        std::fs::write(&path, original).unwrap();
        let config = ConfigLoader::load(&path).unwrap();
        assert_eq!(config.providers["main"].token, "secret-value");
        let state =
            ConfigState::from_config(config, path.clone(), Arc::new(MemoryTokenStore::new()));

        state
            .update_general_settings(update(90, false))
            .await
            .unwrap();

        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(
            saved.contains("token = \"${PIPEDASH_TEST_TOKEN}\""),
            "{saved}"
        );
        assert!(!saved.contains("secret-value"), "{saved}");
        assert!(saved.contains("refresh_interval = 45"));
        assert!(saved.contains("status_pages_enabled = false"));
        assert!(saved.contains("default_refresh_interval = 90"));
    }

    #[tokio::test]
    async fn test_update_general_settings_keeps_the_proxy_reference() {
        let _env = EnvGuard::set(&[("PIPEDASH_TEST_PROXY", Some("http://proxy.corp:3128"))]);
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        std::fs::write(&path, "[general]\nproxy_url = \"${PIPEDASH_TEST_PROXY}\"\n").unwrap();
        let config = ConfigLoader::load(&path).unwrap();
        let state =
            ConfigState::from_config(config, path.clone(), Arc::new(MemoryTokenStore::new()));
        let general = state.get().await.general.clone();
        let save = |proxy_url: &str| GeneralSettingsUpdate {
            metrics_enabled: general.metrics_enabled,
            default_refresh_interval: general.default_refresh_interval,
            status_pages_enabled: !general.status_pages_enabled,
            proxy_url: proxy_url.to_string(),
        };

        state
            .update_general_settings(save(&general.proxy_url))
            .await
            .unwrap();
        let kept = std::fs::read_to_string(&path).unwrap();
        state
            .update_general_settings(save("http://other.corp:8080"))
            .await
            .unwrap();
        let replaced = std::fs::read_to_string(&path).unwrap();

        assert!(
            kept.contains("proxy_url = \"${PIPEDASH_TEST_PROXY}\""),
            "{kept}"
        );
        assert!(
            replaced.contains("proxy_url = \"http://other.corp:8080\""),
            "{replaced}"
        );
    }

    #[tokio::test]
    async fn test_provider_persistence_creates_the_general_table_too() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("config.toml");
        std::fs::write(&path, "# keep me\n[storage]\nbackend = \"sqlite\"\n").unwrap();
        let mut config = PipedashConfig::default();
        config.general.status_pages_enabled = false;
        let state =
            ConfigState::from_config(config, path.clone(), Arc::new(MemoryTokenStore::new()));

        state
            .add_provider(
                "main".to_string(),
                ProviderFileConfig {
                    provider_type: "github".to_string(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# keep me"));
        assert!(saved.contains("[general]"));
        let reloaded = ConfigLoader::load(&path).unwrap();
        assert!(!reloaded.general.status_pages_enabled);
        assert!(reloaded.providers.contains_key("main"));
    }
}
