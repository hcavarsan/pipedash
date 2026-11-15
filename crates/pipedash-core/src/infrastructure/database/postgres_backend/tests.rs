use std::collections::HashMap;

use super::PostgresConfigBackend;
use crate::domain::ProviderConfig;
use crate::infrastructure::config_backend::ConfigBackend;

fn provider(name: &str) -> ProviderConfig {
    ProviderConfig {
        id: None,
        name: name.to_string(),
        provider_type: "stub".to_string(),
        token: String::new(),
        config: HashMap::new(),
        refresh_interval: 30,
        version: None,
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn postgres_marks_and_lists_config_file_providers() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
    let backend = PostgresConfigBackend::new(&url).await.unwrap();
    let suffix = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let from_file = format!("file-{suffix}");
    let from_ui = format!("ui-{suffix}");
    let file_id = backend
        .create_provider(&provider(&from_file))
        .await
        .unwrap();
    let ui_id = backend.create_provider(&provider(&from_ui)).await.unwrap();

    backend
        .mark_config_file_providers(std::slice::from_ref(&from_file))
        .await
        .unwrap();
    let marked = backend.list_config_file_providers().await.unwrap();
    backend.delete_provider(file_id).await.unwrap();
    backend.delete_provider(ui_id).await.unwrap();

    assert!(marked.contains(&(file_id, from_file)));
    assert!(!marked.iter().any(|(id, _)| *id == ui_id));
}
