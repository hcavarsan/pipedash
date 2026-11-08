use super::*;
use crate::application::test_support::Harness;

#[tokio::test]
async fn an_unregistered_provider_type_has_no_metadata() {
    let h = Harness::new().await;
    let mut config = h.config("one", 30);
    config.provider_type = "unregistered".to_string();
    let id = h.repository.add_provider(&config).await.unwrap();

    let error = h
        .provider_service
        .get_provider_table_schema(id)
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::NotFound(_)));
}

#[tokio::test]
async fn checking_permissions_reports_the_plugin_status() {
    let h = Harness::new().await;

    let result = h
        .provider_service
        .check_permissions("stub", Default::default())
        .await
        .unwrap();

    assert!(result.permission_status.is_some());
    assert!(result.features.is_empty());
}
