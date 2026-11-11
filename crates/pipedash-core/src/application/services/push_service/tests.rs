use std::sync::Arc;

use crate::domain::{
    DomainError,
    PushSubscription,
};
use crate::infrastructure::push_relay::RelayOutcome;
use crate::tests::context;
use crate::MemoryTokenStore;

const RELAY: &str = "http://127.0.0.1:9";

fn subscription(relay_subscription_id: &str) -> PushSubscription {
    PushSubscription {
        id: PushSubscription::local_id(relay_subscription_id),
        relay_subscription_id: relay_subscription_id.into(),
        device_name: "Pixel".into(),
        platform: "ios".into(),
        created_at: chrono::Utc::now(),
        last_success_at: None,
    }
}

async fn service(dir: &std::path::Path, relay: Option<&str>) -> super::PushService {
    let core = context(dir, Arc::new(MemoryTokenStore::new())).await;
    core.config_state
        .set_push_relay_url(relay.unwrap_or(""))
        .await
        .unwrap();

    core.push_service()
}

#[tokio::test]
async fn the_configured_relay_is_normalized_and_empty_means_off() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(
        service(dir.path(), Some(" http://relay.example/ "))
            .await
            .relay_url()
            .await
            .as_deref(),
        Some("http://relay.example")
    );
    assert_eq!(service(dir.path(), None).await.relay_url().await, None);
}

#[tokio::test]
async fn turning_the_relay_on_uses_the_default_relay_of_this_build() {
    let dir = tempfile::tempdir().unwrap();
    let official = service(dir.path(), None).await;
    let settings = official.set_relay_enabled(true).await.unwrap();
    assert_eq!(
        settings.relay_url.as_deref(),
        Some(crate::infrastructure::OFFICIAL_PUSH_RELAY_URL)
    );

    let custom = service(dir.path(), None)
        .await
        .with_default_relay("http://dev-relay.example/");
    let settings = custom.set_relay_enabled(true).await.unwrap();
    assert!(settings.enabled);
    assert_eq!(
        settings.relay_url.as_deref(),
        Some("http://dev-relay.example")
    );
    assert_eq!(
        custom.relay_url().await.as_deref(),
        Some("http://dev-relay.example")
    );

    let settings = custom.set_relay_enabled(false).await.unwrap();
    assert!(!settings.enabled);
    assert_eq!(custom.relay_url().await, None);
}

#[tokio::test]
async fn testing_needs_a_relay_and_a_known_subscription() {
    let dir = tempfile::tempdir().unwrap();

    assert!(matches!(
        service(dir.path(), None)
            .await
            .test_subscription("any")
            .await,
        Err(DomainError::Unprocessable { .. })
    ));
    assert!(matches!(
        service(dir.path(), Some(RELAY))
            .await
            .test_subscription("missing")
            .await,
        Err(DomainError::NotFound(_))
    ));
}

#[tokio::test]
async fn an_unreachable_relay_fails_the_test_and_keeps_the_subscription() {
    let dir = tempfile::tempdir().unwrap();
    let service = service(dir.path(), Some(RELAY)).await;
    let saved = subscription("sub-1");
    service
        .subscriptions
        .import(std::slice::from_ref(&saved))
        .await
        .unwrap();

    assert_eq!(
        service.test_subscription(&saved.id).await.unwrap(),
        RelayOutcome::Failed
    );
    assert_eq!(service.list_subscriptions().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_device_push_test_needs_a_relay_and_a_registration() {
    let dir = tempfile::tempdir().unwrap();

    assert!(matches!(
        service(dir.path(), None)
            .await
            .test_device_push("device")
            .await,
        Err(DomainError::Unprocessable {
            code: "PUSH_RELAY_NOT_CONFIGURED",
            ..
        })
    ));
    assert!(matches!(
        service(dir.path(), Some(RELAY))
            .await
            .test_device_push("device")
            .await,
        Err(DomainError::Conflict {
            code: "PUSH_NOT_REGISTERED",
            ..
        })
    ));
}
