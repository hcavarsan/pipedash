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

        if config_path.exists() {
            ConfigLoader::load(&config_path).map_err(|e| {
                DomainError::InvalidConfig(format!("Failed to load config.toml: {}", e))
            })
        } else {
            let platform = Platform::detect();
            ConfigLoader::load_or_create(&config_path, platform)
                .map_err(|e| DomainError::InvalidConfig(format!("Failed to create config: {}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_migrate_creates_config_if_missing() {
        let temp_dir = TempDir::new().unwrap();

        let config = ConfigMigrator::migrate_if_needed(temp_dir.path())
            .await
            .unwrap();

        let toml_path = temp_dir.path().join("config.toml");
        assert!(toml_path.exists());

        assert!(config.general.metrics_enabled);
    }

    #[tokio::test]
    async fn test_migrate_loads_existing_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let content = r#"
[general]
metrics_enabled = false
default_refresh_interval = 120

[storage]
backend = "postgres"

[storage.postgres]
connection_string = "postgres://localhost/test"
"#;
        std::fs::write(&config_path, content).unwrap();

        let config = ConfigMigrator::migrate_if_needed(temp_dir.path())
            .await
            .unwrap();

        assert!(!config.general.metrics_enabled);
        assert_eq!(config.general.default_refresh_interval, 120);
        assert!(config.storage.backend.requires_postgres());
    }
}
