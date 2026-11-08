use std::path::Path;

use super::loader::{
    ConfigLoader,
    Platform,
};
use super::schema::PipedashConfig;
use crate::domain::{
    DomainError,
    DomainResult,
};

pub struct ConfigMigrator;

impl ConfigMigrator {
    pub async fn migrate_if_needed(data_dir: &Path) -> DomainResult<PipedashConfig> {
        let config_path = data_dir.join("config.toml");

        tokio::task::spawn_blocking(move || {
            if config_path.exists() {
                ConfigLoader::load(&config_path).map_err(|e| {
                    DomainError::InvalidConfig(format!("Failed to load config.toml: {}", e))
                })
            } else {
                ConfigLoader::load_or_create(&config_path, Platform::detect()).map_err(|e| {
                    DomainError::InvalidConfig(format!("Failed to create config: {}", e))
                })
            }
        })
        .await
        .map_err(|e| DomainError::InternalError(format!("Config load task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests;
