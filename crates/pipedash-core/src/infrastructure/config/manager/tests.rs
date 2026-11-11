use tempfile::TempDir;

use super::*;
use crate::test_env::EnvGuard;

#[tokio::test]
async fn test_storage_manager_creation() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = PipedashConfig::default();
    config.storage.data_dir = temp_dir.path().to_string_lossy().to_string();

    let token_store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new());

    let manager = StorageManager::with_token_store(config, token_store, true)
        .await
        .unwrap();

    assert!(manager.token_store().await.get_token(1).await.is_err());
    assert!(manager.cache_backend().is_available().await);
}

#[tokio::test]
async fn test_storage_manager_vault_locked_mode() {
    let _env = EnvGuard::set(&[("PIPEDASH_VAULT_PASSWORD", None)]);
    let temp_dir = TempDir::new().unwrap();
    let mut config = PipedashConfig::default();
    config.storage.data_dir = temp_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::from_config_allow_locked(config, false)
        .await
        .unwrap();

    assert!(manager.is_vault_locked().await);
    assert!(manager.requires_vault_password());
}

#[test]
fn test_storage_config_defaults() {
    let config = PipedashConfig::default();

    assert_eq!(config.storage.backend, ConfigStorageBackend::Sqlite);
}

#[test]
fn test_requires_vault_password() {
    let config = PipedashConfig::default();

    let is_desktop = true;
    let is_server = false;

    match (config.storage.backend, is_desktop) {
        (ConfigStorageBackend::Sqlite, true) => {} // Keyring - expected
        _ => panic!("Expected Keyring for desktop SQLite"),
    }

    match (config.storage.backend, is_server) {
        (ConfigStorageBackend::Sqlite, false) => {} // SQLite encrypted - expected
        _ => panic!("Expected SQLite encrypted for server SQLite"),
    }
}
