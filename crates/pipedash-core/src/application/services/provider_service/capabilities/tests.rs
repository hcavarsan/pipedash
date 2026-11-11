use super::*;
use crate::application::test_support::plugins::STUB_HTTP;
use crate::application::test_support::{
    unreachable_url,
    FeedStub,
    Harness,
};

fn http_config(server_url: &str) -> HashMap<String, String> {
    HashMap::from([
        ("server_url".to_string(), server_url.to_string()),
        ("token".to_string(), "token".to_string()),
    ])
}

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

#[tokio::test]
async fn a_permission_probe_the_provider_rejects_reports_an_authentication_failure() {
    let h = Harness::new().await;
    let server = FeedStub::start().await;
    server.respond(401, "{}");

    let error = h
        .provider_service
        .check_permissions(STUB_HTTP, http_config(&server.base_url))
        .await
        .unwrap_err();

    assert!(
        matches!(error, DomainError::ProviderAuthenticationFailed(_)),
        "{error}"
    );
}

#[tokio::test]
async fn a_permission_probe_that_cannot_reach_the_provider_reports_a_network_error() {
    let h = Harness::new().await;

    let error = h
        .provider_service
        .check_permissions(STUB_HTTP, http_config(&unreachable_url()))
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::NetworkError(_)), "{error}");
}
