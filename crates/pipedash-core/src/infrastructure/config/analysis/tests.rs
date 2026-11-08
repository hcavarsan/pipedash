use tempfile::TempDir;

use super::*;
use crate::application::test_support::plugins::registry;
use crate::infrastructure::token_store::MemoryTokenStore;

async fn sqlite_manager(data_dir: &TempDir) -> StorageManager {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = data_dir.path().to_string_lossy().to_string();

    StorageManager::with_token_store(config, Arc::new(MemoryTokenStore::new()), true)
        .await
        .unwrap()
}

#[tokio::test]
async fn switching_sqlite_to_postgres_returns_a_migration_plan() {
    let data_dir = TempDir::new().unwrap();
    let manager = sqlite_manager(&data_dir).await;
    let content = format!(
        "[storage]\ndata_dir = {:?}\nbackend = \"postgres\"\n\n[storage.postgres]\nconnection_string = \"postgres://user:pass@127.0.0.1:1/pipedash\"\n",
        data_dir.path().to_string_lossy()
    );

    let analysis = analyze_config(&content, &manager, None, None, &registry())
        .await
        .unwrap();

    let plan = analysis.migration_plan.expect("migration plan");
    assert_eq!(plan.to.storage.backend, StorageBackend::Postgres);
    assert!(!plan.steps.is_empty());
    assert!(analysis.stats.is_some());
    assert!(
        !analysis
            .postgres_connection
            .expect("connection test")
            .success
    );
}

#[tokio::test]
async fn unchanged_storage_returns_no_migration_plan() {
    let data_dir = TempDir::new().unwrap();
    let manager = sqlite_manager(&data_dir).await;
    let content = format!(
        "[storage]\ndata_dir = {:?}\nbackend = \"sqlite\"\n",
        data_dir.path().to_string_lossy()
    );

    let analysis = analyze_config(&content, &manager, None, None, &registry())
        .await
        .unwrap();

    assert!(analysis.valid);
    assert!(analysis.migration_plan.is_none());
    assert!(analysis.postgres_connection.is_none());
}

#[tokio::test]
async fn analysis_expands_backend_references_before_validation() {
    let data_dir = TempDir::new().unwrap();
    let manager = sqlite_manager(&data_dir).await;
    let content = format!(
        "[storage]\ndata_dir = {:?}\nbackend = \"${{PIPEDASH_ANALYSIS_TEST_BACKEND:-sqlite}}\"\n",
        data_dir.path().to_string_lossy()
    );

    let analysis = analyze_config(&content, &manager, None, None, &registry())
        .await
        .unwrap();

    assert!(analysis.valid);
    assert!(analysis.migration_plan.is_none());
    assert!(analysis.postgres_connection.is_none());
}

#[tokio::test]
async fn analysis_parse_errors_do_not_include_expanded_values() {
    let data_dir = TempDir::new().unwrap();
    let manager = sqlite_manager(&data_dir).await;
    let content =
        "[storage]\nbackend = \"${PIPEDASH_ANALYSIS_TEST_SECRET:-editor-secret-value}\"\n";

    let error = analyze_config(content, &manager, None, None, &registry())
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::InvalidConfig(_)));
    assert!(!error.to_string().contains("editor-secret-value"));
}

#[tokio::test]
async fn invalid_toml_is_an_invalid_config_error() {
    let data_dir = TempDir::new().unwrap();
    let manager = sqlite_manager(&data_dir).await;

    let err = analyze_config("[storage", &manager, None, None, &registry())
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::InvalidConfig(_)));
}

#[tokio::test]
async fn analysis_warns_about_provider_types_missing_from_the_registry() {
    let data_dir = TempDir::new().unwrap();
    let manager = sqlite_manager(&data_dir).await;
    let content = format!(
        "[storage]\ndata_dir = {:?}\n\n[providers.known]\ntype = \"stub\"\n\n[providers.other]\ntype = \"not-registered\"\n",
        data_dir.path().to_string_lossy()
    );

    let analysis = analyze_config(&content, &manager, None, None, &registry())
        .await
        .unwrap();

    let unknown: Vec<_> = analysis
        .warnings
        .iter()
        .filter(|w| w.message.starts_with("Unknown provider type"))
        .map(|w| w.field.as_str())
        .collect();
    assert_eq!(unknown, ["providers.other.type"]);
}
