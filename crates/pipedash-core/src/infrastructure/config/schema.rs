use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{
    Deserialize,
    Serialize,
};

use super::token_ref::TokenReference;

pub(super) const DEFAULT_REFRESH_INTERVAL_SECS: u32 = 30;

pub(super) const DEFAULT_BIND_ADDR_DESKTOP: &str = "127.0.0.1:8080";

pub(super) const DEFAULT_BIND_ADDR_SERVER: &str = "0.0.0.0:8080";

pub(super) const DEFAULT_CORS_ALLOW_ALL: bool = true;

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

    pub fn metrics_db_path(&self) -> PathBuf {
        self.data_dir().join("metrics.db")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir().join("cache")
    }

    pub fn vault_path(&self) -> PathBuf {
        self.data_dir().join("vault.db")
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
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: default_metrics_enabled(),
            default_refresh_interval: default_refresh_interval(),
        }
    }
}

fn default_metrics_enabled() -> bool {
    DEFAULT_METRICS_ENABLED
}

fn default_refresh_interval() -> u32 {
    DEFAULT_REFRESH_INTERVAL_SECS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,

    #[serde(default = "default_cors_allow_all")]
    pub cors_allow_all: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            cors_allow_all: default_cors_allow_all(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn token_reference(&self) -> Result<TokenReference, super::token_ref::TokenRefError> {
        TokenReference::parse(&self.token)
    }

    pub fn has_token(&self) -> bool {
        match self.token_reference() {
            Ok(ref tr) => tr.is_configured(),
            Err(_) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigKey {
    MetricsEnabled,
    DefaultRefreshInterval,
    BindAddr,
    CorsAllowAll,
    DataDir,
    StorageBackend,
    PostgresConnectionString,
}

impl ConfigKey {
    pub fn env_var_name(&self) -> &'static str {
        match self {
            Self::MetricsEnabled => "PIPEDASH_METRICS_ENABLED",
            Self::DefaultRefreshInterval => "PIPEDASH_DEFAULT_REFRESH_INTERVAL",
            Self::BindAddr => "PIPEDASH_BIND_ADDR",
            Self::CorsAllowAll => "PIPEDASH_CORS_ALLOW_ALL",
            Self::DataDir => "PIPEDASH_DATA_DIR",
            Self::StorageBackend => "PIPEDASH_STORAGE_BACKEND",
            Self::PostgresConnectionString => "PIPEDASH_POSTGRES_URL",
        }
    }

    pub fn is_storage_field(&self) -> bool {
        matches!(self, Self::StorageBackend)
    }

    pub fn requires_restart(&self) -> bool {
        matches!(self, Self::StorageBackend | Self::BindAddr | Self::DataDir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PipedashConfig::default();

        assert!(config.general.metrics_enabled);
        assert_eq!(config.general.default_refresh_interval, 30);
        assert_eq!(config.server.bind_addr, "127.0.0.1:8080");
        assert_eq!(config.storage.backend, StorageBackend::Sqlite);
        assert!(config.providers.is_empty());
    }

    #[test]
    fn test_storage_backend_parsing() {
        assert_eq!(
            "sqlite".parse::<StorageBackend>().unwrap(),
            StorageBackend::Sqlite
        );
        assert_eq!(
            "postgres".parse::<StorageBackend>().unwrap(),
            StorageBackend::Postgres
        );
        assert_eq!(
            "postgresql".parse::<StorageBackend>().unwrap(),
            StorageBackend::Postgres
        );
    }

    #[test]
    fn test_storage_backend_helpers() {
        assert!(StorageBackend::Postgres.requires_postgres());
        assert!(!StorageBackend::Sqlite.requires_postgres());
        assert!(StorageBackend::Sqlite.is_sqlite());
        assert!(!StorageBackend::Postgres.is_sqlite());
    }

    #[test]
    fn test_storage_config_validation() {
        let sqlite_config = StorageConfig::default();
        assert!(sqlite_config.validate().is_ok());

        let pg_config = StorageConfig {
            backend: StorageBackend::Postgres,
            ..Default::default()
        };
        assert!(pg_config.validate().is_err());

        let pg_config_with_conn = StorageConfig {
            backend: StorageBackend::Postgres,
            postgres: PostgresConfig {
                connection_string: "postgres://localhost/test".to_string(),
            },
            ..Default::default()
        };
        assert!(pg_config_with_conn.validate().is_ok());
    }

    #[test]
    fn test_provider_token_reference() {
        let provider = ProviderFileConfig {
            name: Some("Test Provider".to_string()),
            provider_type: "github".to_string(),
            token: "${GITHUB_TOKEN}".to_string(),
            refresh_interval: 30,
            config: HashMap::new(),
        };

        let token_ref = provider.token_reference().unwrap();
        assert_eq!(
            token_ref,
            TokenReference::EnvVar("GITHUB_TOKEN".to_string())
        );
        assert!(provider.has_token());
    }

    #[test]
    fn test_provider_display_name() {
        let with_name = ProviderFileConfig {
            name: Some("My GitHub".to_string()),
            provider_type: "github".to_string(),
            ..Default::default()
        };
        assert_eq!(with_name.display_name("github-main"), "My GitHub");

        let without_name = ProviderFileConfig {
            name: None,
            provider_type: "github".to_string(),
            ..Default::default()
        };
        assert_eq!(without_name.display_name("github-main"), "github-main");
    }

    #[test]
    fn test_config_key_env_vars() {
        assert_eq!(
            ConfigKey::MetricsEnabled.env_var_name(),
            "PIPEDASH_METRICS_ENABLED"
        );
        assert_eq!(
            ConfigKey::StorageBackend.env_var_name(),
            "PIPEDASH_STORAGE_BACKEND"
        );
        assert_eq!(
            ConfigKey::PostgresConnectionString.env_var_name(),
            "PIPEDASH_POSTGRES_URL"
        );
    }

    #[test]
    fn test_config_paths() {
        let config = PipedashConfig::default();
        let data_dir = config.data_dir();

        assert!(config.db_path().starts_with(&data_dir));
        assert!(config.metrics_db_path().starts_with(&data_dir));
        assert!(config.cache_dir().starts_with(&data_dir));
        assert!(config.vault_path().starts_with(&data_dir));
    }
}
