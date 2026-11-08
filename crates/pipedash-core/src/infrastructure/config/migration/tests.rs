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
