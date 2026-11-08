use std::sync::Arc;

use tempfile::TempDir;

use super::*;
use crate::application::test_support::plugins::registry;
use crate::infrastructure::database::SqliteConfigBackend;
use crate::infrastructure::MemoryTokenStore;

async fn create_test_backend() -> (SqliteConfigBackend, Arc<MemoryTokenStore>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let pool = crate::infrastructure::database::init_database(db_path)
        .await
        .unwrap();
    let backend = SqliteConfigBackend::new(pool);
    let token_store = Arc::new(MemoryTokenStore::new());
    (backend, token_store, temp_dir)
}

#[tokio::test]
async fn test_sync_toml_to_db_new_provider() {
    let (backend, token_store, _temp) = create_test_backend().await;
    let sync_service = ProviderSyncService::new();

    let mut toml_providers = IndexMap::new();
    toml_providers.insert(
        "test-stub".to_string(),
        ProviderFileConfig {
            name: None,
            provider_type: "stub".to_string(),
            token: "${STUB_TOKEN}".to_string(),
            refresh_interval: 30,
            config: Default::default(),
        },
    );

    let result = sync_service
        .sync_toml_to_db(
            toml_providers,
            &backend,
            token_store.as_ref(),
            true,
            &registry(),
        )
        .await
        .unwrap();

    assert_eq!(result.added.len(), 1);
    assert_eq!(result.added[0], "test-stub");
    assert!(result.updated.is_empty());
    assert!(result.removed.is_empty());

    let db_providers = backend.list_providers().await.unwrap();
    assert_eq!(db_providers.len(), 1);
    assert_eq!(db_providers[0].name, "test-stub");
}

#[tokio::test]
async fn test_sync_toml_to_db_update_provider() {
    let (backend, token_store, _temp) = create_test_backend().await;
    let sync_service = ProviderSyncService::new();

    let initial_provider = ProviderConfig {
        id: None,
        name: "test-stub".to_string(),
        provider_type: "stub".to_string(),
        token: "${OLD_TOKEN}".to_string(),
        config: Default::default(),
        refresh_interval: 30,
        version: None,
    };
    backend.create_provider(&initial_provider).await.unwrap();

    let mut updated_toml = IndexMap::new();
    updated_toml.insert(
        "test-stub".to_string(),
        ProviderFileConfig {
            name: None,
            provider_type: "stub".to_string(),
            token: "${NEW_TOKEN}".to_string(),
            refresh_interval: 60,
            config: Default::default(),
        },
    );

    let result = sync_service
        .sync_toml_to_db(
            updated_toml,
            &backend,
            token_store.as_ref(),
            true,
            &registry(),
        )
        .await
        .unwrap();

    assert!(result.added.is_empty());
    assert_eq!(result.updated.len(), 1);
    assert_eq!(result.updated[0], "test-stub");
    assert!(result.removed.is_empty());

    let db_providers = backend.list_providers().await.unwrap();
    assert_eq!(db_providers[0].token, "${NEW_TOKEN}");
    assert_eq!(db_providers[0].refresh_interval, 60);
}

#[tokio::test]
async fn test_sync_toml_to_db_remove_provider() {
    let (backend, token_store, _temp) = create_test_backend().await;
    let sync_service = ProviderSyncService::new();

    let initial_provider = ProviderConfig {
        id: None,
        name: "test-stub".to_string(),
        provider_type: "stub".to_string(),
        token: "${TOKEN}".to_string(),
        config: Default::default(),
        refresh_interval: 30,
        version: None,
    };
    backend.create_provider(&initial_provider).await.unwrap();

    let result = sync_service
        .sync_toml_to_db(
            IndexMap::new(),
            &backend,
            token_store.as_ref(),
            true,
            &registry(),
        )
        .await
        .unwrap();

    assert!(result.added.is_empty());
    assert!(result.updated.is_empty());
    assert_eq!(result.removed.len(), 1);
    assert_eq!(result.removed[0], "test-stub");

    let db_providers = backend.list_providers().await.unwrap();
    assert!(db_providers.is_empty());
}

#[tokio::test]
async fn test_sync_toml_to_db_keep_orphans_for_postgres() {
    let (backend, token_store, _temp) = create_test_backend().await;
    let sync_service = ProviderSyncService::new();

    let db_only_provider = ProviderConfig {
        id: None,
        name: "db-only-provider".to_string(),
        provider_type: "stub".to_string(),
        token: "${DB_TOKEN}".to_string(),
        config: Default::default(),
        refresh_interval: 30,
        version: None,
    };
    backend.create_provider(&db_only_provider).await.unwrap();

    let result = sync_service
        .sync_toml_to_db(
            IndexMap::new(),
            &backend,
            token_store.as_ref(),
            false,
            &registry(),
        )
        .await
        .unwrap();

    assert!(result.added.is_empty());
    assert!(result.updated.is_empty());
    assert!(result.removed.is_empty());

    let db_providers = backend.list_providers().await.unwrap();
    assert_eq!(db_providers.len(), 1);
    assert_eq!(db_providers[0].name, "db-only-provider");
}

#[tokio::test]
async fn test_sync_toml_to_db_postgres_mode_adds_from_toml() {
    let (backend, token_store, _temp) = create_test_backend().await;
    let sync_service = ProviderSyncService::new();

    let db_only_provider = ProviderConfig {
        id: None,
        name: "db-only-provider".to_string(),
        provider_type: "stub".to_string(),
        token: "${DB_TOKEN}".to_string(),
        config: Default::default(),
        refresh_interval: 30,
        version: None,
    };
    backend.create_provider(&db_only_provider).await.unwrap();

    let mut toml_providers = IndexMap::new();
    toml_providers.insert(
        "toml-provider".to_string(),
        ProviderFileConfig {
            name: None,
            provider_type: "stub".to_string(),
            token: "${TOML_TOKEN}".to_string(),
            refresh_interval: 60,
            config: Default::default(),
        },
    );

    let result = sync_service
        .sync_toml_to_db(
            toml_providers,
            &backend,
            token_store.as_ref(),
            false,
            &registry(),
        )
        .await
        .unwrap();

    assert_eq!(result.added.len(), 1);
    assert_eq!(result.added[0], "toml-provider");
    assert!(result.updated.is_empty());
    assert!(result.removed.is_empty());

    let db_providers = backend.list_providers().await.unwrap();
    assert_eq!(db_providers.len(), 2);
}

#[tokio::test]
async fn test_sync_toml_to_db_stores_an_empty_token_when_none_resolves() {
    let (backend, token_store, _temp) = create_test_backend().await;

    let mut toml_providers = IndexMap::new();
    toml_providers.insert(
        "ci-stub".to_string(),
        ProviderFileConfig {
            name: Some("CI Stub".to_string()),
            provider_type: "stub".to_string(),
            token: String::new(),
            refresh_interval: 60,
            config: Default::default(),
        },
    );

    ProviderSyncService::new()
        .sync_toml_to_db(
            toml_providers,
            &backend,
            token_store.as_ref(),
            true,
            &registry(),
        )
        .await
        .unwrap();

    let id = backend.list_providers().await.unwrap()[0].id.unwrap();
    assert_eq!(token_store.get_token(id).await.unwrap(), "");
}

#[test]
fn test_export_toml_generated_var_never_reuses_preserved_var() {
    let provider = |id: i64, name: &str, token: &str| ProviderConfig {
        id: Some(id),
        name: name.to_string(),
        provider_type: "stub".to_string(),
        token: token.to_string(),
        config: HashMap::new(),
        refresh_interval: 30,
        version: None,
    };
    let preserved = provider(1, "other", "${PIPEDASH_TOKEN_PROD}");
    let literal = provider(2, "prod", "stubtok_literal_secret");

    for providers in [
        [preserved.clone(), literal.clone()],
        [
            literal.clone(),
            ProviderConfig {
                id: Some(3),
                ..preserved
            },
        ],
    ] {
        let exported = ProviderSyncService::export_toml(&providers).unwrap();

        assert!(!exported.contains("stubtok_literal_secret"));
        assert_eq!(exported.matches("${PIPEDASH_TOKEN_PROD:-}").count(), 1);
        assert!(exported.contains("${PIPEDASH_TOKEN_PROD_2:-}"));
    }
}

#[test]
fn test_export_toml_is_providers_only() {
    let exported = ProviderSyncService::export_toml(&[]).unwrap();

    assert!(!exported.contains("[general]"));
    assert!(exported.starts_with("# Pipedash providers export."));
}
