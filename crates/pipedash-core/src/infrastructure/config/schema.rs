use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::{
    PipelineStatus,
    ViewDisplay,
};

pub mod applications;

use applications::{
    ApplicationFileConfig,
    CorrelationFileConfig,
};

pub(super) const DEFAULT_REFRESH_INTERVAL_SECS: u32 = 30;

pub const MIN_REFRESH_INTERVAL_SECS: u32 = 5;

pub const MAX_REFRESH_INTERVAL_SECS: u32 = 300;

pub(super) const DEFAULT_BIND_ADDR_DESKTOP: &str = "127.0.0.1:8080";

pub(super) const DEFAULT_BIND_ADDR_SERVER: &str = "0.0.0.0:8080";

pub(super) const DEFAULT_CORS_ALLOW_ALL: bool = false;

pub(super) const DEFAULT_METRICS_ENABLED: bool = true;

pub(super) const DEFAULT_DATA_DIR_SERVER: &str = "./data";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    #[default]
    Sqlite,
    Postgres,
}

impl StorageBackend {
    pub fn requires_postgres(&self) -> bool {
        matches!(self, Self::Postgres)
    }

    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite)
    }
}

impl std::fmt::Display for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite => write!(f, "sqlite"),
            Self::Postgres => write!(f, "postgres"),
        }
    }
}

impl std::str::FromStr for StorageBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sqlite" => Ok(Self::Sqlite),
            "postgres" | "postgresql" => Ok(Self::Postgres),
            _ => Err(format!(
                "Unknown storage backend: {}. Valid options: sqlite, postgres",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipedashConfig {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub storage: StorageConfig,

    #[serde(default)]
    pub providers: IndexMap<String, ProviderFileConfig>,

    #[serde(default, skip_serializing_if = "PushConfig::is_unset")]
    pub push: PushConfig,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub views: IndexMap<String, ViewFileConfig>,

    #[serde(default, skip_serializing_if = "CorrelationFileConfig::is_default")]
    pub correlation: CorrelationFileConfig,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub applications: IndexMap<String, ApplicationFileConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ViewFileConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_contains: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<PipelineStatus>,
    #[serde(flatten)]
    pub display: ViewDisplay,
}

impl PipedashConfig {
    pub fn data_dir(&self) -> PathBuf {
        if self.storage.data_dir.is_empty() {
            Self::default_data_dir()
        } else {
            PathBuf::from(&self.storage.data_dir)
        }
    }

    pub fn default_data_dir() -> PathBuf {
        std::env::var("PIPEDASH_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::data_dir()
                    .map(|p| p.join("pipedash"))
                    .unwrap_or_else(|| PathBuf::from(".pipedash"))
            })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir().join("pipedash.db")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir().join("cache")
    }

    pub fn config_path(&self) -> PathBuf {
        self.data_dir().join("config.toml")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_metrics_enabled")]
    pub metrics_enabled: bool,

    #[serde(default = "default_refresh_interval")]
    pub default_refresh_interval: u32,

    #[serde(default = "default_true")]
    pub status_pages_enabled: bool,

    #[serde(default)]
    pub proxy_url: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: default_metrics_enabled(),
            default_refresh_interval: default_refresh_interval(),
            status_pages_enabled: default_true(),
            proxy_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralSettingsUpdate {
    pub metrics_enabled: bool,
    pub default_refresh_interval: u32,
    pub status_pages_enabled: bool,
    pub proxy_url: String,
}

fn default_metrics_enabled() -> bool {
    DEFAULT_METRICS_ENABLED
}

fn default_refresh_interval() -> u32 {
    DEFAULT_REFRESH_INTERVAL_SECS
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,

    #[serde(default = "default_cors_allow_all")]
    pub cors_allow_all: bool,

    #[serde(default)]
    pub base_path: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            cors_allow_all: default_cors_allow_all(),
            base_path: String::new(),
        }
    }
}

fn default_bind_addr() -> String {
    DEFAULT_BIND_ADDR_DESKTOP.to_string()
}

fn default_cors_allow_all() -> bool {
    DEFAULT_CORS_ALLOW_ALL
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data_dir: String,

    #[serde(default)]
    pub backend: StorageBackend,

    #[serde(default, skip_serializing_if = "is_default_postgres_config")]
    pub postgres: PostgresConfig,

    #[serde(default, skip_serializing)]
    pub vault_password: Option<String>,
}

fn is_default_postgres_config(c: &PostgresConfig) -> bool {
    c.connection_string.is_empty()
}

impl StorageConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.backend.requires_postgres() && self.postgres.connection_string.is_empty() {
            return Err("PostgreSQL backend selected but connection_string is empty".to_string());
        }

        Ok(())
    }

    pub fn summary(&self) -> String {
        format!("Storage: {} backend", self.backend)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostgresConfig {
    #[serde(default)]
    pub connection_string: String,
}

pub const OFFICIAL_PUSH_RELAY_URL: &str = "https://relay.pipedash.app";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PushConfig {
    #[serde(default)]
    pub relay_url: String,
}

impl PushConfig {
    fn is_unset(&self) -> bool {
        self.relay_url.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderFileConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(rename = "type")]
    pub provider_type: String,

    #[serde(default)]
    pub token: String,

    #[serde(default = "default_provider_refresh")]
    pub refresh_interval: u32,

    #[serde(default)]
    pub config: HashMap<String, String>,
}

impl Default for ProviderFileConfig {
    fn default() -> Self {
        Self {
            name: None,
            provider_type: String::new(),
            token: String::new(),
            refresh_interval: default_provider_refresh(),
            config: HashMap::new(),
        }
    }
}

fn default_provider_refresh() -> u32 {
    DEFAULT_REFRESH_INTERVAL_SECS
}

impl ProviderFileConfig {
    pub fn display_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.name.as_deref().unwrap_or(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigKey {
    MetricsEnabled,
    DefaultRefreshInterval,
    StatusPagesEnabled,
    BindAddr,
    CorsAllowAll,
    DataDir,
    StorageBackend,
    PostgresConnectionString,
    PushRelayUrl,
}

impl ConfigKey {
    pub fn env_var_name(&self) -> &'static str {
        match self {
            Self::MetricsEnabled => "PIPEDASH_METRICS_ENABLED",
            Self::DefaultRefreshInterval => "PIPEDASH_DEFAULT_REFRESH_INTERVAL",
            Self::StatusPagesEnabled => "PIPEDASH_STATUS_PAGES_ENABLED",
            Self::BindAddr => "PIPEDASH_BIND_ADDR",
            Self::CorsAllowAll => "PIPEDASH_CORS_ALLOW_ALL",
            Self::DataDir => "PIPEDASH_DATA_DIR",
            Self::StorageBackend => "PIPEDASH_STORAGE_BACKEND",
            Self::PostgresConnectionString => "PIPEDASH_POSTGRES_URL",
            Self::PushRelayUrl => "PIPEDASH_PUSH_RELAY_URL",
        }
    }
}

#[cfg(test)]
mod tests;
