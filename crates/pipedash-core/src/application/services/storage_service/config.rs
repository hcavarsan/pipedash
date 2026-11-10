use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use pipedash_plugin_api::PluginRegistry;
use serde::Serialize;

use super::blocking;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::event::EventBus;
use crate::infrastructure::{
    ConfigAnalysis,
    ConfigLoader,
    ConfigValidator,
    PipedashConfig,
    StorageManager,
};
use crate::CoreContext;

#[derive(Debug, Serialize)]
pub struct StorageConfigResponse {
    pub config: PipedashConfig,
    pub summary: String,
}

impl StorageConfigResponse {
    pub fn new(config: PipedashConfig) -> Self {
        let summary = config.storage.summary();
        Self { config, summary }
    }
}

#[derive(Debug, Serialize)]
pub struct ConfigContentResponse {
    pub content: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct StoragePathsResponse {
    pub config_file: String,
    pub pipedash_db: String,
    pub metrics_db: String,
    pub data_dir: String,
    pub cache_dir: String,
    pub vault_path: String,
}

impl StoragePathsResponse {
    pub fn new(config_file: &Path, data_dir: &Path) -> Self {
        let display = |path: &Path| path.display().to_string();
        Self {
            config_file: display(config_file),
            pipedash_db: display(&data_dir.join("pipedash.db")),
            metrics_db: display(&data_dir.join("metrics.db")),
            data_dir: display(data_dir),
            cache_dir: display(&data_dir.join("cache")),
            vault_path: display(&data_dir.join("vault")),
        }
    }
}

pub async fn load_config(data_dir: &Path) -> DomainResult<PipedashConfig> {
    let config_path = data_dir.join("config.toml");
    blocking(move || {
        if !config_path.exists() {
            return Err(DomainError::ConfigNotFound);
        }
        ConfigLoader::load(&config_path).map_err(|e| DomainError::InvalidConfigFile {
            path: config_path.display().to_string(),
            errors: e.to_string(),
        })
    })
    .await?
}

pub async fn effective_data_dir(config_file: &Path, fallback: &Path) -> DomainResult<PathBuf> {
    let (config_file, fallback) = (config_file.to_path_buf(), fallback.to_path_buf());
    blocking(move || {
        if !config_file.exists() {
            return fallback;
        }
        ConfigLoader::load(&config_file)
            .map(|config| config.data_dir())
            .unwrap_or(fallback)
    })
    .await
}

pub async fn read_config_content(path: &Path) -> DomainResult<ConfigContentResponse> {
    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
        DomainError::InternalError(format!(
            "Failed to read config file at {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(ConfigContentResponse {
        content,
        path: path.display().to_string(),
    })
}

pub fn validate_config(config: &PipedashConfig, registry: &PluginRegistry) -> DomainResult<()> {
    let validation = ConfigValidator::validate_with_plugins(config, registry);
    if validation.is_ok() {
        return Ok(());
    }

    Err(DomainError::InvalidConfig(format!(
        "Config validation failed: {}",
        validation.error_summary()
    )))
}

pub async fn save_config_content(
    path: &Path, content: &str, registry: &PluginRegistry, core: Option<&CoreContext>,
) -> DomainResult<()> {
    let owned = content.to_owned();
    let config = blocking(move || ConfigLoader::parse(&owned))
        .await?
        .map_err(|_| {
            DomainError::InvalidConfig(
                "Invalid config fields or environment references".to_string(),
            )
        })?;
    validate_config(&config, registry)?;

    tokio::fs::write(path, content).await.map_err(|e| {
        DomainError::InternalError(format!(
            "Failed to save config file at {}: {}",
            path.display(),
            e
        ))
    })?;
    tracing::info!("Config file saved: {:?}", path);

    let Some(core) = core else {
        return Ok(());
    };
    core.apply_config_file().await.map_err(|e| {
        DomainError::InternalError(format!(
            "config.toml was saved, but its providers couldn't be applied: {e}"
        ))
    })?;

    Ok(())
}

pub async fn analyze_config(
    content: &str, manager: &StorageManager, event_bus: Option<Arc<dyn EventBus>>,
    registry: &PluginRegistry,
) -> DomainResult<ConfigAnalysis> {
    crate::infrastructure::analyze_config(content, manager, event_bus, None, registry).await
}

#[cfg(test)]
mod tests;
