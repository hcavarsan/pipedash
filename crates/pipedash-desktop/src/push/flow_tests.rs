use std::path::Path;
use std::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering,
};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::json;

use super::stub::{
    Gate,
    Reply,
    Stub,
};
use super::*;

const API_TOKEN: &str = "api-token";
const HOST: &str = "127.0.0.1";

async fn relay_stub(
    tag: &'static str, put_status: impl Fn(&str) -> u16 + Send + Sync + 'static,
) -> Stub {
    let created = AtomicUsize::new(0);
    Stub::start(move |request| match request.method.as_str() {
        "POST" if request.path == "/v1/subscriptions" => {
            let number = created.fetch_add(1, Ordering::SeqCst) + 1;
            Reply::json(
                201,
                json!({ "subscription_id": format!("{tag}-relay-{number}") }),
            )
        }
        "PUT" => Reply::empty(put_status(&request.path)),
        "DELETE" => Reply::empty(204),
        _ => Reply::empty(500),
    })
    .await
}

async fn server_stub(register_gate: Option<Arc<Gate>>) -> Stub {
    Stub::start(move |request| match request.method.as_str() {
        "POST" if request.path.ends_with("/api/v1/push/subscriptions") => {
            let subscription = request.body["subscription_id"].as_str().unwrap_or_default();
            let reply = Reply::json(201, json!({ "id": format!("srv-{subscription}") }));
            match &register_gate {
                Some(gate) => reply.held_by(gate),
                None => reply,
            }
        }
        "DELETE" => Reply::empty(204),
        _ => Reply::empty(500),
    })
    .await
}

fn seed(dir: &Path, url: &str, id: &str, relay_subscription: &str) -> ServerRecord {
    let record = ServerRecord {
        id: id.to_owned(),
        url: url.to_owned(),
        label: "CI".to_owned(),
        paired_at: Utc::now(),
    };
    store::set_secret(Secret::ApiToken, id, API_TOKEN).unwrap();
    store::set_secret(Secret::RelaySubscription, id, relay_subscription).unwrap();
    store::update(dir, |file| file.servers.push(record.clone())).unwrap();
    record
}

fn assert_no_secrets(id: &str) {
    assert_eq!(
        store::get_secret(Secret::ApiToken, id).unwrap(),
        None,
        "{id}"
    );
    assert_eq!(
        store::get_secret(Secret::RelaySubscription, id).unwrap(),
        None,
        "{id}"
    );
}

#[tokio::test]
async fn reset_waits_for_an_in_flight_resync_and_leaves_no_pairing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Arc::new(Gate::default());
    let relay = relay_stub("race", |_| 404).await;
    let server = server_stub(Some(Arc::clone(&gate))).await;
    seed(dir.path(), &server.url, "race-old", "race-stale");

    let sync = tokio::spawn({
        let (dir, relay_url) = (dir.path().to_path_buf(), relay.url.clone());
        async move { sync_servers(&dir, &relay_url, "device-token").await }
    });
    gate.arrived.notified().await;

    let reset = tokio::spawn({
        let (dir, relay_url) = (dir.path().to_path_buf(), relay.url.clone());
        async move { reset_with(&dir, Some(&relay_url)).await }
    });
    let reset_ran_during_resync = server
        .saw(
            "DELETE",
            "/api/v1/push/subscriptions/race-old",
            Duration::from_millis(500),
        )
        .await;
    gate.release.notify_one();
    sync.await.unwrap().unwrap();
    reset.await.unwrap().unwrap();

    assert!(!reset_ran_during_resync);
    assert!(store::read(dir.path()).unwrap().servers.is_empty());
    assert!(!dir.path().join("push_servers.json").exists());
    assert_no_secrets("race-old");
    assert_no_secrets("srv-race-relay-1");
}

async fn assert_lost_relay_subscription_is_replaced(tag: &'static str, status: u16) {
    let dir = tempfile::tempdir().unwrap();
    let relay = relay_stub(tag, move |_| status).await;
    let server = server_stub(None).await;
    let old_id = format!("{tag}-old");
    let stale = format!("{tag}-stale");
    let record = seed(dir.path(), &server.url, &old_id, &stale);

    sync_servers(dir.path(), &relay.url, "device-token")
        .await
        .unwrap();

    let new_relay = format!("{tag}-relay-1");
    let new_id = format!("srv-{new_relay}");
    assert_eq!(relay.count("PUT", &format!("/v1/subscriptions/{stale}")), 1);
    let created: Vec<_> = relay
        .requests()
        .into_iter()
        .filter(|request| request.method == "POST")
        .collect();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].body["device_token"], "device-token");

    let registered: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|request| request.method == "POST")
        .collect();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].body["subscription_id"], new_relay.as_str());

    let servers = store::read(dir.path()).unwrap().servers;
    assert_eq!(
        servers,
        vec![ServerRecord {
            id: new_id.clone(),
            ..record
        }]
    );
    assert_eq!(
        store::get_secret(Secret::RelaySubscription, &new_id).unwrap(),
        Some(new_relay)
    );
    assert_eq!(
        store::get_secret(Secret::ApiToken, &new_id).unwrap(),
        Some(API_TOKEN.to_owned())
    );
    assert_no_secrets(&old_id);
    assert_eq!(
        server.count("DELETE", &format!("/api/v1/push/subscriptions/{old_id}")),
        1
    );
}

#[tokio::test]
async fn a_relay_404_recreates_the_subscription_and_persists_the_new_capability() {
    assert_lost_relay_subscription_is_replaced("gone404", 404).await;
}

#[tokio::test]
async fn a_relay_410_recreates_the_subscription_and_persists_the_new_capability() {
    assert_lost_relay_subscription_is_replaced("gone410", 410).await;
}

#[tokio::test]
async fn every_sync_rotates_every_server_after_a_failure_and_after_a_new_pairing() {
    let dir = tempfile::tempdir().unwrap();
    let failing = Arc::new(AtomicBool::new(true));
    let relay = relay_stub("rot", {
        let failing = Arc::clone(&failing);
        move |path| {
            if path.ends_with("rot-relay-a") && failing.load(Ordering::SeqCst) {
                500
            } else {
                204
            }
        }
    })
    .await;
    let server = server_stub(None).await;
    seed(
        dir.path(),
        &format!("{}/a", server.url),
        "rot-a",
        "rot-relay-a",
    );
    seed(
        dir.path(),
        &format!("{}/b", server.url),
        "rot-b",
        "rot-relay-b",
    );

    sync_servers(dir.path(), &relay.url, "device-token")
        .await
        .unwrap();
    assert_eq!(relay.count("PUT", "/v1/subscriptions/rot-relay-a"), 1);
    assert_eq!(relay.count("PUT", "/v1/subscriptions/rot-relay-b"), 1);

    failing.store(false, Ordering::SeqCst);
    let url = format!("{}/c", server.url);
    let pairing = Pairing {
        url: &url,
        host: HOST,
        api_token: API_TOKEN,
        label: "C",
        device_token: "device-token",
    };
    establish(dir.path(), &relay.url, &pairing, None)
        .await
        .unwrap();

    sync_servers(dir.path(), &relay.url, "device-token")
        .await
        .unwrap();
    assert_eq!(relay.count("PUT", "/v1/subscriptions/rot-relay-a"), 2);
    assert_eq!(relay.count("PUT", "/v1/subscriptions/rot-relay-b"), 2);
    assert_eq!(relay.count("PUT", "/v1/subscriptions/rot-relay-1"), 1);
    assert_eq!(store::read(dir.path()).unwrap().servers.len(), 3);
    for put in relay.requests().iter().filter(|r| r.method == "PUT") {
        assert_eq!(put.body["device_token"], "device-token");
    }
}

#[tokio::test]
async fn reset_preserves_unreadable_pairing_state_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("push_servers.json");
    std::fs::write(&path, "invalid-json").unwrap();

    assert!(reset_with(dir.path(), None).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid-json");

    std::fs::write(&path, "{}").unwrap();
    reset_with(dir.path(), None).await.unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn reset_retains_the_pairing_record_when_local_secret_deletion_fails() {
    let dir = tempfile::tempdir().unwrap();
    let id = "reset-local-secret-failure";
    store::update(dir.path(), |file| {
        file.servers.push(ServerRecord {
            id: id.into(),
            url: "https://unused.example".into(),
            label: "Local cleanup".into(),
            paired_at: chrono::Utc::now(),
        })
    })
    .unwrap();
    store::set_secret(Secret::RelaySubscription, id, "relay-subscription").unwrap();
    store::fail_next_delete(Secret::RelaySubscription, id);

    assert!(reset_with(dir.path(), None).await.is_err());
    assert_eq!(store::read(dir.path()).unwrap().servers[0].id, id);
    assert!(store::get_secret(Secret::RelaySubscription, id)
        .unwrap()
        .is_some());

    reset_with(dir.path(), None).await.unwrap();
    assert!(!dir.path().join("push_servers.json").exists());
    assert_no_secrets(id);
}
