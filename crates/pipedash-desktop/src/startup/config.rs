use std::path::Path;

use pipedash_core::infrastructure::config::{
    ConfigLoader,
    PipedashConfig,
};

pub(crate) fn ensure_desktop_config(
    app_data_dir: &Path, config_path: &Path,
) -> Result<bool, String> {
    if config_path.exists() {
        return Ok(false);
    }

    let legacy_db_path = app_data_dir.join("pipedash.db");

    let is_legacy_migration = if legacy_db_path.exists() {
        let metadata = std::fs::metadata(&legacy_db_path)
            .map_err(|e| format!("Failed to check legacy database: {}", e))?;

        if metadata.len() > 0 {
            tracing::info!(
                "Detected legacy database at {} ({} bytes) - preserving existing data",
                legacy_db_path.display(),
                metadata.len()
            );
            true
        } else {
            tracing::debug!("Legacy database exists but is empty - treating as fresh install");
            false
        }
    } else {
        false
    };

    let mut config = PipedashConfig::default();
    config.general.default_refresh_interval = 300;
    config.storage.data_dir = app_data_dir.display().to_string();

    let config_toml =
        ConfigLoader::to_toml(&config).map_err(|e| format!("Failed to serialize config: {}", e))?;

    let (title, note) = if is_legacy_migration {
        (
            "Auto-generated configuration for legacy database",
            "This config was automatically created to maintain backward compatibility",
        )
    } else {
        (
            "Pipedash Desktop Configuration",
            "Desktop uses SQLite with system keyring for secure credential storage",
        )
    };

    let config_content = format!(
        "# {}\n# Created: {}\n# {}\n\n{}",
        title,
        chrono::Utc::now().to_rfc3339(),
        note,
        config_toml
    );

    std::fs::write(config_path, &config_content)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    if is_legacy_migration {
        tracing::info!(
            "Successfully created config.toml for legacy database at {}",
            config_path.display()
        );
        tracing::info!("Existing database and credentials will be preserved");
    } else {
        tracing::info!(
            "Successfully created default config.toml at {}",
            config_path.display()
        );
        tracing::info!("Desktop initialized with SQLite + System Keyring (default)");
    }

    Ok(true)
}

#[cfg(any(mobile, test))]
pub(crate) fn follow_app_container(config_path: &Path) -> Result<bool, String> {
    if !config_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let mut config =
        ConfigLoader::parse_raw(&content).map_err(|e| format!("Failed to parse config: {}", e))?;
    if config.storage.data_dir.is_empty() {
        return Ok(false);
    }

    config.storage.data_dir.clear();
    ConfigLoader::save(&config, config_path)
        .map_err(|e| format!("Failed to save config: {}", e))?;
    Ok(true)
}
