use std::collections::HashMap;

use super::SqliteConfigBackend;
use crate::domain::ProviderConfig;
use crate::infrastructure::config_backend::{
    ConfigBackend,
    StoredPermissions,
};
use crate::infrastructure::database::init_database;

#[tokio::test]
async fn test_sqlite_backend_providers() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_database(db_path).await.unwrap();
    let backend = SqliteConfigBackend::new(pool);

    let config = ProviderConfig {
        id: None,
        name: "Test Provider".to_string(),
        provider_type: "stub".to_string(),
        config: HashMap::from([("owner".to_string(), "test".to_string())]),
        token: String::new(),
        refresh_interval: 30,
        version: None,
    };

    let id = backend.create_provider(&config).await.unwrap();
    assert!(id > 0);

    let retrieved = backend.get_provider(id).await.unwrap().unwrap();
    assert_eq!(retrieved.name, "Test Provider");
    assert_eq!(retrieved.provider_type, "stub");

    let all = backend.list_providers().await.unwrap();
    assert_eq!(all.len(), 1);

    let updated_config = ProviderConfig {
        id: Some(id),
        name: "Updated Provider".to_string(),
        provider_type: "stub".to_string(),
        config: HashMap::from([("owner".to_string(), "updated".to_string())]),
        token: String::new(),
        refresh_interval: 60,
        version: None,
    };
    backend.update_provider(id, &updated_config).await.unwrap();

    let retrieved = backend.get_provider(id).await.unwrap().unwrap();
    assert_eq!(retrieved.name, "Updated Provider");
    assert_eq!(retrieved.refresh_interval, 60);

    backend.delete_provider(id).await.unwrap();
    assert!(backend.get_provider(id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_sqlite_backend_table_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_database(db_path).await.unwrap();
    let backend = SqliteConfigBackend::new(pool);

    let config = ProviderConfig {
        id: None,
        name: "Prefs Provider".to_string(),
        provider_type: "stub".to_string(),
        config: HashMap::new(),
        token: String::new(),
        refresh_interval: 30,
        version: None,
    };
    let provider_id = backend.create_provider(&config).await.unwrap();

    let prefs_json = r#"{"columns":["name","status"],"sort":"name"}"#;
    backend
        .set_table_preferences(provider_id, "pipelines", prefs_json)
        .await
        .unwrap();

    let retrieved = backend
        .get_table_preferences(provider_id, "pipelines")
        .await
        .unwrap();
    assert_eq!(retrieved, Some(prefs_json.to_string()));

    let missing = backend
        .get_table_preferences(provider_id, "other")
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_sqlite_backend_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_database(db_path).await.unwrap();
    let backend = SqliteConfigBackend::new(pool);

    let config = ProviderConfig {
        id: None,
        name: "Perms Provider".to_string(),
        provider_type: "stub".to_string(),
        config: HashMap::new(),
        token: String::new(),
        refresh_interval: 30,
        version: None,
    };
    let provider_id = backend.create_provider(&config).await.unwrap();

    let mut perms_map = HashMap::new();
    perms_map.insert("repo:read".to_string(), true);
    perms_map.insert("repo:write".to_string(), false);
    let permissions = StoredPermissions {
        permissions: perms_map,
        last_checked: chrono::Utc::now(),
    };
    backend
        .store_permissions(provider_id, &permissions)
        .await
        .unwrap();

    let exported = backend.export_all().await.unwrap();
    let retrieved = &exported.permissions[&provider_id];
    assert_eq!(retrieved.permissions.get("repo:read"), Some(&true));
    assert_eq!(retrieved.permissions.get("repo:write"), Some(&false));
}

#[tokio::test]
async fn test_sqlite_backend_export_import() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_database(db_path).await.unwrap();
    let backend = SqliteConfigBackend::new(pool);

    let config = ProviderConfig {
        id: None,
        name: "Export Test".to_string(),
        provider_type: "stub".to_string(),
        config: HashMap::from([("host".to_string(), "host.example.test".to_string())]),
        token: String::new(),
        refresh_interval: 45,
        version: None,
    };
    backend.create_provider(&config).await.unwrap();

    let export = backend.export_all().await.unwrap();
    assert_eq!(export.providers.len(), 1);
    assert_eq!(export.providers[0].name, "Export Test");

    let dir2 = tempfile::tempdir().unwrap();
    let db_path2 = dir2.path().join("test2.db");
    let pool2 = init_database(db_path2).await.unwrap();
    let backend2 = SqliteConfigBackend::new(pool2);

    backend2.import_all(&export).await.unwrap();

    let imported = backend2.list_providers().await.unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].name, "Export Test");
}
