use std::path::{
    Path,
    PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use toml_edit::{
    DocumentMut,
    Item,
};

use super::interpolation::{
    interpolate_toml,
    InterpolationError,
};
use super::schema::PipedashConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStatus {
    pub config_exists: bool,
    pub config_valid: bool,
    pub validation_errors: Vec<String>,
    pub needs_setup: bool,
    pub needs_migration: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_path: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigLoadError {
    #[error("Config file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Environment variable interpolation failed: {0}")]
    InterpolationError(#[from] InterpolationError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type ConfigLoadResult<T> = Result<T, ConfigLoadError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Server,
}

impl Platform {
    pub fn detect() -> Self {
        if std::env::var("PIPEDASH_SERVER_MODE").is_ok()
            || std::env::var("PIPEDASH_API_MODE").is_ok()
        {
            return Self::Server;
        }

        if std::env::var("KUBERNETES_SERVICE_HOST").is_ok()
            || std::env::var("DOCKER_CONTAINER").is_ok()
            || std::env::var("PIPEDASH_DATA_DIR")
                .map(|d| d.starts_with("/data") || d.starts_with("/var"))
                .unwrap_or(false)
        {
            return Self::Server;
        }

        Self::Desktop
    }
}

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn discover_config_path() -> PathBuf {
        if let Ok(path) = std::env::var("PIPEDASH_CONFIG_PATH") {
            tracing::debug!("Using config path from PIPEDASH_CONFIG_PATH: {}", path);
            return PathBuf::from(path);
        }

        let platform = Platform::detect();

        match platform {
            Platform::Desktop => {
                if let Some(config_dir) = dirs::config_dir() {
                    let path = config_dir.join("pipedash").join("config.toml");
                    if path.exists() {
                        tracing::debug!("Using desktop config path: {}", path.display());
                        return path;
                    }
                }
            }
            Platform::Server => {
                let path = PathBuf::from("/etc/pipedash/config.toml");
                if path.exists() {
                    tracing::debug!("Using server config path: {}", path.display());
                    return path;
                }
            }
        }

        let fallback = PipedashConfig::default_data_dir().join("config.toml");
        tracing::debug!("Using fallback config path: {}", fallback.display());
        fallback
    }

    pub fn load_default() -> ConfigLoadResult<PipedashConfig> {
        let path = Self::discover_config_path();
        Self::load(&path)
    }

    pub fn load(path: &Path) -> ConfigLoadResult<PipedashConfig> {
        if !path.exists() {
            return Err(ConfigLoadError::FileNotFound(path.to_path_buf()));
        }

        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    pub fn load_or_create(path: &Path, platform: Platform) -> ConfigLoadResult<PipedashConfig> {
        if path.exists() {
            Self::load(path)
        } else {
            let config = Self::create_default_config(platform);

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            Self::save(&config, path)?;
            tracing::info!("Created default config at {:?}", path);

            Ok(config)
        }
    }

    fn create_default_config(platform: Platform) -> PipedashConfig {
        use super::schema::{
            DEFAULT_BIND_ADDR_SERVER,
            DEFAULT_DATA_DIR_SERVER,
        };

        let mut config = PipedashConfig::default();

        match platform {
            Platform::Desktop => {}
            Platform::Server => {
                config.server.bind_addr = DEFAULT_BIND_ADDR_SERVER.to_string();
                config.storage.data_dir = DEFAULT_DATA_DIR_SERVER.to_string();
            }
        }

        config
    }

    pub fn parse(content: &str) -> ConfigLoadResult<PipedashConfig> {
        let mut value: toml::Value = toml::from_str(content)?;

        interpolate_toml(&mut value)?;

        let config: PipedashConfig = value.try_into().map_err(|e| {
            ConfigLoadError::InvalidConfig(format!("Failed to deserialize config: {}", e))
        })?;

        tracing::debug!(
            backend = %config.storage.backend,
            "Loaded config"
        );

        Ok(config)
    }

    pub fn parse_raw(content: &str) -> ConfigLoadResult<PipedashConfig> {
        let config: PipedashConfig = toml::from_str(content)?;
        Ok(config)
    }

    pub fn config_exists(path: &Path) -> bool {
        path.exists()
    }

    pub fn get_setup_status(data_dir: &Path) -> SetupStatus {
        let config_path = data_dir.join("config.toml");
        let legacy_path = data_dir.join("storage_config.json");

        if legacy_path.exists() && !config_path.exists() {
            return SetupStatus {
                config_exists: false,
                config_valid: false,
                validation_errors: vec![],
                needs_setup: false,
                needs_migration: true,
                database_exists: None,
                database_path: None,
            };
        }

        if !config_path.exists() {
            return SetupStatus {
                config_exists: false,
                config_valid: false,
                validation_errors: vec![],
                needs_setup: true,
                needs_migration: false,
                database_exists: None,
                database_path: None,
            };
        }

        match Self::load(&config_path) {
            Ok(config) => {
                let validation = config.validate();
                let errors: Vec<String> = validation.errors.iter().map(|e| e.to_string()).collect();

                let db_path = config.db_path();
                let db_exists = db_path.exists();

                SetupStatus {
                    config_exists: true,
                    config_valid: errors.is_empty(),
                    validation_errors: errors,
                    needs_setup: false,
                    needs_migration: false,
                    database_exists: Some(db_exists),
                    database_path: Some(db_path.display().to_string()),
                }
            }
            Err(e) => SetupStatus {
                config_exists: true,
                config_valid: false,
                validation_errors: vec![format!("Failed to parse config: {}", e)],
                needs_setup: false,
                needs_migration: false,
                database_exists: None,
                database_path: None,
            },
        }
    }

    pub fn to_toml(config: &PipedashConfig) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(config)
    }

    pub fn save(config: &PipedashConfig, path: &Path) -> ConfigLoadResult<()> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let mut doc = content.parse::<DocumentMut>().map_err(|e| {
                ConfigLoadError::InvalidConfig(format!("Failed to parse existing config: {}", e))
            })?;

            Self::update_document(&mut doc, config)?;

            std::fs::write(path, doc.to_string())?;
        } else {
            let toml_str = Self::to_toml(config).map_err(|e| {
                ConfigLoadError::InvalidConfig(format!("Failed to serialize config: {}", e))
            })?;

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::write(path, toml_str)?;
        }

        Ok(())
    }

    fn update_document(doc: &mut DocumentMut, config: &PipedashConfig) -> ConfigLoadResult<()> {
        if let Some(general) = doc.get_mut("general").and_then(|v| v.as_table_like_mut()) {
            general.insert(
                "metrics_enabled",
                Item::Value(config.general.metrics_enabled.into()),
            );
            general.insert(
                "default_refresh_interval",
                Item::Value((config.general.default_refresh_interval as i64).into()),
            );
        }

        if let Some(server) = doc.get_mut("server").and_then(|v| v.as_table_like_mut()) {
            server.insert(
                "bind_addr",
                Item::Value(config.server.bind_addr.clone().into()),
            );
            server.insert(
                "cors_allow_all",
                Item::Value(config.server.cors_allow_all.into()),
            );
        }

        if doc.get("storage").is_none() {
            doc.insert("storage", Item::Table(toml_edit::Table::new()));
        }

        if let Some(storage) = doc.get_mut("storage").and_then(|v| v.as_table_like_mut()) {
            storage.insert(
                "backend",
                Item::Value(config.storage.backend.to_string().into()),
            );

            if !config.storage.data_dir.is_empty() {
                storage.insert(
                    "data_dir",
                    Item::Value(config.storage.data_dir.clone().into()),
                );
            }

            if !config.storage.postgres.connection_string.is_empty() {
                if storage.get("postgres").is_none() {
                    storage.insert("postgres", Item::Table(toml_edit::Table::new()));
                }
                if let Some(postgres) = storage
                    .get_mut("postgres")
                    .and_then(|v| v.as_table_like_mut())
                {
                    postgres.insert(
                        "connection_string",
                        Item::Value(config.storage.postgres.connection_string.clone().into()),
                    );
                }
            }
        }

        Ok(())
    }

    pub fn get_sources(path: &Path) -> ConfigLoadResult<ConfigSources> {
        let content = if path.exists() {
            std::fs::read_to_string(path)?
        } else {
            String::new()
        };

        let file_config = if !content.is_empty() {
            Some(Self::parse_raw(&content)?)
        } else {
            None
        };

        let resolved = if path.exists() {
            Self::load(path)?
        } else {
            PipedashConfig::default()
        };

        Ok(ConfigSources {
            resolved,
            file_config,
            config_path: path.to_path_buf(),
        })
    }
}

#[derive(Debug)]
pub struct ConfigSources {
    pub resolved: PipedashConfig,
    pub file_config: Option<PipedashConfig>,
    pub config_path: PathBuf,
}

impl ConfigSources {
    pub fn get_source(&self, key: &str) -> ValueSource {
        let env_var_name = match key {
            "general.metrics_enabled" => Some("PIPEDASH_METRICS_ENABLED"),
            "general.default_refresh_interval" => Some("PIPEDASH_DEFAULT_REFRESH_INTERVAL"),
            "server.bind_addr" => Some("PIPEDASH_BIND_ADDR"),
            "storage.data_dir" => Some("PIPEDASH_DATA_DIR"),
            "storage.backend" => Some("PIPEDASH_STORAGE_BACKEND"),
            "storage.postgres.connection_string" => Some("PIPEDASH_POSTGRES_URL"),
            _ => None,
        };

        if let Some(var) = env_var_name {
            if std::env::var(var).is_ok() {
                return ValueSource::EnvVar(var.to_string());
            }
        }

        if self.file_config.is_some() {
            return ValueSource::ConfigFile(self.config_path.clone());
        }

        ValueSource::Default
    }
}

#[derive(Debug, Clone)]
pub enum ValueSource {
    EnvVar(String),
    ConfigFile(PathBuf),
    Default,
}

impl std::fmt::Display for ValueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnvVar(name) => write!(f, "env: {}", name),
            Self::ConfigFile(path) => write!(f, "file: {}", path.display()),
            Self::Default => write!(f, "default"),
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_platform_detect_default() {
        let platform = Platform::detect();
        assert!(matches!(platform, Platform::Desktop | Platform::Server));
    }

    #[test]
    fn test_parse_simple_config() {
        let content = r#"
[general]
metrics_enabled = true
default_refresh_interval = 60

[storage]
backend = "sqlite"
"#;

        let config = ConfigLoader::parse(content).unwrap();
        assert!(config.general.metrics_enabled);
        assert_eq!(config.general.default_refresh_interval, 60);
    }

    #[test]
    fn test_parse_with_providers() {
        let content = r#"
[providers.my-github]
name = "My GitHub"
type = "github"
token = "GITHUB_TOKEN"
refresh_interval = 30

[providers.my-github.config]
base_url = ""
"#;

        let config = ConfigLoader::parse(content).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert!(config.providers.contains_key("my-github"));
        assert_eq!(
            config.providers["my-github"].name,
            Some("My GitHub".to_string())
        );
        assert_eq!(config.providers["my-github"].provider_type, "github");
    }

    #[test]
    fn test_load_or_create() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = ConfigLoader::load_or_create(&config_path, Platform::Desktop).unwrap();
        assert!(config_path.exists());
        assert!(config.general.metrics_enabled);

        let config2 = ConfigLoader::load(&config_path).unwrap();
        assert!(config2.general.metrics_enabled);
    }

    #[test]
    fn test_env_var_interpolation() {
        std::env::set_var("TEST_BIND_ADDR", "0.0.0.0:9999");

        let content = r#"
[server]
bind_addr = "${TEST_BIND_ADDR}"
"#;

        let config = ConfigLoader::parse(content).unwrap();
        assert_eq!(config.server.bind_addr, "0.0.0.0:9999");

        std::env::remove_var("TEST_BIND_ADDR");
    }

    #[test]
    fn test_env_var_with_default() {
        let content = r#"
[server]
bind_addr = "${NONEXISTENT_VAR:-127.0.0.1:8080}"
"#;

        let config = ConfigLoader::parse(content).unwrap();
        assert_eq!(config.server.bind_addr, "127.0.0.1:8080");
    }

    #[test]
    fn test_to_toml() {
        let config = PipedashConfig::default();
        let toml_str = ConfigLoader::to_toml(&config).unwrap();

        assert!(toml_str.contains("[general]"));
        assert!(toml_str.contains("metrics_enabled"));
    }

    #[test]
    fn test_discover_config_path_env_override() {
        std::env::set_var("PIPEDASH_CONFIG_PATH", "/custom/path/config.toml");
        let path = ConfigLoader::discover_config_path();
        assert_eq!(path, PathBuf::from("/custom/path/config.toml"));
        std::env::remove_var("PIPEDASH_CONFIG_PATH");
    }
}
