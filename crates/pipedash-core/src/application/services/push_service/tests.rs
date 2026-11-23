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

async fn webhook_stub() -> (String, tokio::task::JoinHandle<String>) {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hooks/team", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            request.extend_from_slice(&buffer[..count]);
            let text = String::from_utf8_lossy(&request).to_string();
            let Some((head, body)) = text.split_once("\r\n\r\n") else {
                continue;
            };
            let length: usize = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap()
                .parse()
                .unwrap();
            if count == 0 || body.len() >= length {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await
                    .unwrap();
                return format!("{}\n{body}", head.lines().next().unwrap());
            }
        }
    });
    (url, server)
}

fn failed_pipeline() -> crate::domain::Pipeline {
    serde_json::from_value(serde_json::json!({
        "id": "stub__1__acme/app/ci.yml",
        "provider_id": 1,
        "provider_type": "stub",
        "name": "CI",
        "status": "failed",
        "last_run": null,
        "last_updated": chrono::Utc::now(),
        "repository": "acme/app",
        "branch": "main",
        "workflow_file": null,
    }))
    .unwrap()
}

#[tokio::test]
async fn a_failure_is_posted_to_the_team_webhook_without_a_push_relay() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    let (url, server) = webhook_stub().await;
    std::fs::write(
        core.config_state.config_path(),
        format!("[push]\nfailure_webhook_url = \"{url}\"\n"),
    )
    .unwrap();
    core.config_state.reload().await.unwrap();
    core.config_state.set_push_relay_url("").await.unwrap();
    core.config_state.reload().await.unwrap();

    core.push_service()
        .notify_failures(&[failed_pipeline()])
        .await
        .unwrap();
    let request = tokio::time::timeout(std::time::Duration::from_secs(15), server)
        .await
        .unwrap()
        .unwrap();
    let (head, body) = request.split_once('\n').unwrap();
    let message: serde_json::Value = serde_json::from_str(body).unwrap();

    assert!(head.starts_with("POST /hooks/team "));
    assert_eq!(message["pipeline"], "CI");
    assert_eq!(message["provider"], "stub");
    assert_eq!(message["branch"], "main");
    assert!(message["text"].as_str().unwrap().starts_with("CI failed"));
}
