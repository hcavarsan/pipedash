use std::path::PathBuf;
use std::sync::{
    Arc,
    OnceLock,
};

use pipedash_plugin_api::PluginRegistry;
use tokio::sync::RwLock;

use super::loader::{
    ConfigLoader,
    Platform,
};
use super::schema::{
    ConfigKey,
    GeneralSettingsUpdate,
    PipedashConfig,
    MAX_REFRESH_INTERVAL_SECS,
    MIN_REFRESH_INTERVAL_SECS,
};
use super::sync::ProviderSyncService;
use super::validation::{
    ConfigValidator,
    ValidationResult,
};
use crate::application::{
    HealthHandle,
    HealthInput,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::http_client::parse_proxy_url;
use crate::infrastructure::TokenStore;

pub struct ConfigState {
    config: RwLock<Arc<PipedashConfig>>,

    config_path: PathBuf,

    token_store: Arc<dyn TokenStore>,

    platform: Platform,

    sync_service: ProviderSyncService,

    health: OnceLock<HealthHandle>,

    plugin_registry: Arc<PluginRegistry>,
}

impl ConfigState {
    pub async fn initialize(
        data_dir: &std::path::Path, token_store: Arc<dyn TokenStore>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> DomainResult<Arc<Self>> {
        let platform = Platform::detect();

        let config_path = ConfigLoader::discover_config_path();
        let legacy_path = data_dir.join("storage_config.json");

        if legacy_path.exists() && !config_path.exists() {
            let migrated_config =
                super::migration::ConfigMigrator::migrate_if_needed(data_dir).await?;
            return Ok(Self::from_config(
                migrated_config,
                config_path,
                token_store,
                plugin_registry,
            ));
        }

        let path = config_path.clone();
        let config =
            tokio::task::spawn_blocking(move || ConfigLoader::load_or_create(&path, platform))
                .await
                .map_err(|e| DomainError::InternalError(format!("Config load task failed: {e}")))?
                .map_err(|e| DomainError::InvalidConfig(format!("Failed to load config: {}", e)))?;

        let validation = ConfigValidator::validate_with_plugins(&config, &plugin_registry);
        for error in &validation.errors {
            tracing::error!("Config error: {}", error);
        }
        for warning in &validation.warnings {
            tracing::warn!("Config warning: {}", warning);
        }

        let state = Arc::new(Self {
            config: RwLock::new(Arc::new(config)),
            config_path,
            token_store,
            platform,
            sync_service: ProviderSyncService::new(),
            health: OnceLock::new(),
            plugin_registry,
        });

        Ok(state)
    }

    pub fn from_config(
        config: PipedashConfig, config_path: PathBuf, token_store: Arc<dyn TokenStore>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: RwLock::new(Arc::new(config)),
            config_path,
            token_store,
            platform: Platform::detect(),
            sync_service: ProviderSyncService::new(),
            health: OnceLock::new(),
            plugin_registry,
        })
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

    pub fn attach_health(&self, health: HealthHandle) {
        let _ = self.health.set(health);
    }

    fn notify_settings_changed(&self) {
        if let Some(health) = self.health.get() {
            health.send(HealthInput::SettingsChanged);
        }
    }

    pub async fn reload(&self) -> DomainResult<()> {
        let path = self.config_path.clone();
        let new_config = tokio::task::spawn_blocking(move || ConfigLoader::load(&path))
            .await
            .map_err(|e| DomainError::InternalError(format!("Config load task failed: {e}")))?
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to reload config: {}", e)))?;

        let validation = ConfigValidator::validate_with_plugins(&new_config, &self.plugin_registry);
        if !validation.errors.is_empty() {
            return Err(DomainError::InvalidConfig(format!(
                "Config validation failed: {:?}",
                validation.errors
            )));
        }

        *self.config.write().await = Arc::new(new_config);

        self.notify_settings_changed();

        tracing::info!("Config reloaded from {:?}", self.config_path);
        Ok(())
    }

    pub async fn validate(&self) -> ValidationResult {
        ConfigValidator::validate_with_plugins(&*self.config.read().await, &self.plugin_registry)
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

        let mut config = (*self.get().await).clone();

        config.general.metrics_enabled = update.metrics_enabled;
        config.general.default_refresh_interval = update.default_refresh_interval;
        config.general.status_pages_enabled = update.status_pages_enabled;
        config.general.proxy_url = proxy_url;

        self.persist_general(&config).await?;

        self.notify_settings_changed();

        Ok(())
    }

    pub async fn set_push_relay_url(&self, relay_url: &str) -> DomainResult<()> {
        let mut config = (*self.get().await).clone();

        config.push.relay_url = relay_url.trim().trim_end_matches('/').to_string();

        self.persist_edit(&config, |doc| {
            if config.push.relay_url.is_empty() {
                doc.remove("push");
            } else {
                doc.entry("push").or_insert_with(toml_edit::table)["relay_url"] =
                    toml_edit::value(config.push.relay_url.as_str());
            }
        })
        .await?;

        Ok(())
    }

    pub fn push_relay_from_env() -> bool {
        std::env::var(ConfigKey::PushRelayUrl.env_var_name())
            .is_ok_and(|value| !value.trim().is_empty())
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
}

mod persistence;
mod providers;

pub use providers::RemovedProvider;

#[cfg(test)]
mod tests;
