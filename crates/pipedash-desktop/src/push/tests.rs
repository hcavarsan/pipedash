use serde_json::json;

use super::stub::Stub;
use super::*;
use crate::push::store::Secret;

#[test]
fn maps_relay_status_codes() {
    assert!(matches!(status_error(404), PushError::RelayGone));
    assert!(matches!(status_error(410), PushError::RelayGone));
    assert!(matches!(status_error(429), PushError::RateLimited));
    assert!(matches!(status_error(503), PushError::Unreachable));
    assert!(matches!(status_error(401), PushError::Unexpected(401)));
}

#[tokio::test]
async fn a_rotated_subscription_the_relay_forgot_is_reported_as_gone() {
    let relay = Stub::start(|request| match request.path.as_str() {
        "/v1/subscriptions/gone" => 410,
        "/v1/subscriptions/down" => 502,
        _ => 204,
    })
    .await;

    assert!(relay::rotate(&relay.url, "live", "token").await.is_ok());
    assert!(matches!(
        relay::rotate(&relay.url, "gone", "token").await,
        Err(PushError::RelayGone)
    ));
    assert!(matches!(
        relay::rotate(&relay.url, "down", "token").await,
        Err(PushError::Unreachable)
    ));
    assert!(relay::delete(&relay.url, "gone").await.is_ok());
    assert!(relay::delete(&relay.url, "down").await.is_err());
}

fn seed(dir: &Path, servers: &[(&str, &str)]) {
    for (id, subscription) in servers {
        store::set_secret(Secret::ApiToken, id, "api-token");
        store::set_secret(Secret::RelaySubscription, id, subscription);
    }
    let records: Vec<_> = servers.iter().map(|(id, _)| json!({ "id": id })).collect();
    std::fs::write(
        dir.join("push_servers.json"),
        json!({ "servers": records }).to_string(),
    )
    .unwrap();
}

fn secrets(id: &str) -> (Option<String>, Option<String>) {
    (
        store::get_secret(Secret::ApiToken, id).unwrap(),
        store::get_secret(Secret::RelaySubscription, id).unwrap(),
    )
}

#[tokio::test]
async fn legacy_cleanup_keeps_a_record_until_the_relay_confirms() {
    let dir = tempfile::tempdir().unwrap();
    seed(
        dir.path(),
        &[("cleanup-ok", "sub-ok"), ("cleanup-down", "sub-down")],
    );
    let flaky = Stub::start(|request| {
        if request.path.ends_with("sub-down") {
            503
        } else {
            204
        }
    })
    .await;

    cleanup_legacy(dir.path(), &flaky.url).await;

    assert_eq!(secrets("cleanup-ok"), (None, None));
    assert_eq!(
        secrets("cleanup-down"),
        (Some("api-token".to_owned()), Some("sub-down".to_owned()))
    );
    assert!(dir.path().join("push_servers.json").exists());

    let healthy = Stub::start(|_| 204).await;
    cleanup_legacy(dir.path(), &healthy.url).await;

    assert_eq!(secrets("cleanup-down"), (None, None));
    assert!(!dir.path().join("push_servers.json").exists());
    assert_eq!(flaky.count("DELETE", "/v1/subscriptions/sub-ok"), 1);
    assert_eq!(healthy.count("DELETE", "/v1/subscriptions/sub-down"), 1);
    assert_eq!(healthy.count("DELETE", "/v1/subscriptions/sub-ok"), 0);
}

#[tokio::test]
async fn acknowledging_tells_the_host_which_alerts_were_shown() {
    let (host, alert_host) = flow_paired().await;
    host.core
        .device_pushes()
        .set(&alert_host.device_id, "sub-1")
        .await
        .unwrap();

    assert!(acknowledge(&alert_host, 5).await);
    assert!(
        !acknowledge(
            &AlertHost {
                server_url: "http://127.0.0.1:1".to_owned(),
                ..alert_host
            },
            5
        )
        .await
    );
}

async fn flow_paired() -> (crate::remote::test_host::Host, AlertHost) {
    let host =
        crate::remote::test_host::start_host_with_relay(Some("https://relay.example.com")).await;
    let code = pipedash_web::pairing::create_pairing(&host.state, &host.url.url)
        .unwrap()
        .code;
    let alert_host = crate::remote::alerts::claim_alert_host(host.url.clone(), &code, true)
        .await
        .unwrap();

    (host, alert_host)
}
