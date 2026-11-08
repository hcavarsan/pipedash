use std::path::{
    Path,
    PathBuf,
};

use pipedash_plugin_api::PluginRegistry;
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

use super::interpolation::{
    interpolate_toml,
    InterpolationError,
};
use super::schema::PipedashConfig;
use super::validation::ConfigValidator;

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

    pub fn get_setup_status(data_dir: &Path, registry: &PluginRegistry) -> SetupStatus {
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
                let validation = ConfigValidator::validate_with_plugins(&config, registry);
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
}

mod persistence;

#[cfg(test)]
mod tests;
