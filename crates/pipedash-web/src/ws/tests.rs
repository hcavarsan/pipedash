use std::sync::Arc;
use std::time::Duration;

use axum::routing::get;
use axum::Router;
use pipedash_core::application::ObservedNotification;
use pipedash_core::domain::NotificationPreferences;
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};
use pipedash_core::CoreContext;
use serde_json::{
    json,
    Value,
};
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::{
    TcpListener,
    TcpStream,
};

use super::connection::MAX_ADMITTED;
use super::event_bus::WebSocketEventBus;
use super::*;
use crate::test_support::hosted;

async fn read_frame(stream: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).await.ok()?;
    let len = match head[1] & 0x7f {
        126 => stream.read_u16().await.ok()? as usize,
        127 => stream.read_u64().await.ok()? as usize,
        len => len as usize,
    };
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await.ok()?;
    Some((head[0] & 0x0f, payload))
}

async fn upgrade(addr: std::net::SocketAddr, path: &str) -> TcpStream {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let handshake = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(handshake.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    while !response.ends_with(b"\r\n\r\n") {
        response.push(stream.read_u8().await.unwrap());
    }
    assert!(response.starts_with(b"HTTP/1.1 101"));
    tokio::time::sleep(Duration::from_millis(100)).await;

    stream
}

async fn connect_state(state: AppState) -> TcpStream {
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    upgrade(addr, "/ws").await
}

async fn connect(bus: &Arc<WebSocketEventBus>) -> TcpStream {
    connect_state(AppState::setup_mode(
        bus.clone(),
        crate::test_support::registry(),
    ))
    .await
}

async fn send_json(stream: &mut TcpStream, value: Value) {
    let payload = value.to_string().into_bytes();
    let mask = [7u8, 11, 13, 17];
    let mut frame = vec![0x81];
    match payload.len() {
        len if len < 126 => frame.push(0x80 | len as u8),
        len => {
            frame.push(0x80 | 126);
            frame.extend((len as u16).to_be_bytes());
        }
    }
    frame.extend(mask);
    frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    stream.write_all(&frame).await.unwrap();
}

async fn next_message(stream: &mut TcpStream) -> Value {
    let (opcode, payload) = tokio::time::timeout(Duration::from_secs(5), read_frame(stream))
        .await
        .expect("no message arrived")
        .expect("the socket closed");
    assert_eq!(opcode, 0x1);
    serde_json::from_slice(&payload).unwrap()
}

async fn assert_closed(stream: &mut TcpStream) {
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match read_frame(stream).await {
                None | Some((0x8, _)) => break,
                Some(_) => {}
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the socket stayed open");
}

async fn claim(stream: &mut TcpStream, code: &str) -> Value {
    send_json(
        stream,
        json!({ "type": "claim", "code": code, "name": "Pixel", "platform": "android" }),
    )
    .await;
    next_message(stream).await
}

#[tokio::test]
async fn lagged_client_gets_closed_so_it_reconnects() {
    let bus = Arc::new(WebSocketEventBus::new());
    let mut stream = connect(&bus).await;

    for _ in 0..1100 {
        bus.emit(CoreEvent::ProvidersChanged).await;
    }

    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match read_frame(&mut stream).await {
                None => break,
                Some((0x8, _)) => break,
                Some(_) => {}
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "socket stayed open after the client lagged");
}

#[tokio::test]
async fn live_run_events_reach_clients_with_their_payloads() {
    let bus = Arc::new(WebSocketEventBus::new());
    let mut stream = connect(&bus).await;
    let run: pipedash_core::domain::PipelineRun = serde_json::from_value(serde_json::json!({
        "id": "p__3", "pipeline_id": "p", "run_number": 3, "status": "running",
        "started_at": "2026-01-01T00:00:00Z", "duration_seconds": 12, "logs_url": ""
    }))
    .unwrap();

    bus.emit(CoreEvent::RunLogTreeChanged {
        pipeline_id: "p".into(),
        run_number: 3,
    })
    .await;
    bus.emit(CoreEvent::RunUpdated {
        pipeline_id: "p".into(),
        run_number: 3,
        run: Box::new(run.clone()),
    })
    .await;

    assert_eq!(
        next_message(&mut stream).await,
        serde_json::json!({
            "type": "run-log-tree-changed",
            "payload": { "pipelineId": "p", "runNumber": 3 },
        })
    );
    assert_eq!(
        next_message(&mut stream).await,
        serde_json::json!({
            "type": "run-updated",
            "payload": {
                "pipelineId": "p",
                "runNumber": 3,
                "run": serde_json::to_value(&run).unwrap(),
            },
        })
    );
}

#[tokio::test]
async fn a_browser_auth_frame_gets_auth_ok_when_no_token_is_required() {
    let bus = Arc::new(WebSocketEventBus::new());
    let mut stream = connect(&bus).await;

    send_json(&mut stream, json!({ "type": "auth", "token": "anything" })).await;

    assert_eq!(
        next_message(&mut stream).await,
        json!({
            "type": "auth-ok",
            "payload": { "host": "server", "name": null, "deviceId": null },
        })
    );
}

#[tokio::test]
async fn the_operator_token_authenticates_and_requests_carry_it() {
    let state = AppState::setup_mode(
        Arc::new(WebSocketEventBus::new()),
        crate::test_support::registry(),
    );
    state.set_api_token("vault-password");

    let mut stream = connect_state(state.clone()).await;
    send_json(&mut stream, json!({ "type": "auth", "token": "wrong" })).await;
    let rejected = next_message(&mut stream).await;
    assert_eq!(rejected["type"], "auth-error");
    assert_eq!(rejected["payload"]["code"], "AUTH_FAILED");
    assert_closed(&mut stream).await;

    let mut stream = connect_state(state).await;
    send_json(
        &mut stream,
        json!({ "type": "auth", "token": "vault-password" }),
    )
    .await;
    assert_eq!(next_message(&mut stream).await["type"], "auth-ok");
    send_json(
        &mut stream,
        json!({ "type": "request", "id": 5, "method": "GET", "path": "/providers" }),
    )
    .await;
    let response = next_message(&mut stream).await;

    assert_eq!(response["type"], "response");
    assert_eq!(response["id"], 5);
    assert_eq!(response["status"], 503);
}

#[tokio::test]
async fn a_request_before_authenticating_is_refused() {
    let state = AppState::setup_mode(
        Arc::new(WebSocketEventBus::new()),
        crate::test_support::registry(),
    );
    state.set_api_token("vault-password");
    let mut stream = connect_state(state).await;

    send_json(
        &mut stream,
        json!({ "type": "request", "id": 1, "method": "GET", "path": "/providers" }),
    )
    .await;

    assert_eq!(
        next_message(&mut stream).await["payload"]["code"],
        "AUTH_FAILED"
    );
    assert_closed(&mut stream).await;
}

#[tokio::test]
async fn a_claim_pairs_the_device_and_serves_requests_then_the_token_reconnects() {
    let host = hosted("ws-claim").await;
    let (code, _) = host.state.pairing_codes.issue().unwrap();
    let mut stream = connect_state(host.state.clone()).await;

    let ok = claim(&mut stream, &code).await;

    assert_eq!(ok["type"], "auth-ok");
    assert_eq!(ok["payload"]["host"], "desktop");
    assert_eq!(ok["payload"]["name"], "test-mac");
    let device_id = ok["payload"]["deviceId"].as_str().unwrap().to_string();
    let token = ok["payload"]["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 64);

    send_json(
        &mut stream,
        json!({ "type": "request", "id": 1, "method": "GET", "path": "/devices" }),
    )
    .await;
    let response = next_message(&mut stream).await;
    assert_eq!(response["id"], 1);
    assert_eq!(response["status"], 200);
    let body: Value = serde_json::from_str(response["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["devices"][0]["id"], device_id);
    assert_eq!(body["devices"][0]["current"], true);

    let mut again = connect_state(host.state.clone()).await;
    send_json(&mut again, json!({ "type": "auth", "token": token })).await;
    let ok = next_message(&mut again).await;
    assert_eq!(ok["payload"]["deviceId"], device_id);
    assert!(ok["payload"].get("token").is_none());

    let mut stranger = connect_state(host.state.clone()).await;
    send_json(&mut stranger, json!({ "type": "auth", "token": "unknown" })).await;
    assert_eq!(
        next_message(&mut stranger).await["payload"]["code"],
        "AUTH_FAILED"
    );
}

#[tokio::test]
async fn stopping_the_host_closes_connected_sockets() {
    let state = AppState::hosted(
        Arc::new(WebSocketEventBus::new()),
        None,
        "test-mac",
        crate::test_support::registry(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(crate::serve_host(listener, state, async move {
        let _ = stopped.await;
    }));
    let mut stream = upgrade(addr, "/api/v1/ws").await;

    stop.send(()).unwrap();

    assert_closed(&mut stream).await;
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("serve_host kept running")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn a_busy_socket_rejects_excess_requests_and_still_closes() {
    let host = crate::test_support::server("ws-busy").await;
    let mut stream = connect_state(host.state.clone()).await;
    let blocked = host.state.inner.write().await;

    for id in 1..=MAX_ADMITTED as u64 + 1 {
        send_json(
            &mut stream,
            json!({ "type": "request", "id": id, "method": "GET", "path": "/providers" }),
        )
        .await;
    }

    let rejected = next_message(&mut stream).await;
    assert_eq!(rejected["id"], MAX_ADMITTED as u64 + 1);
    assert_eq!(rejected["status"], 429);

    host.state.closing.send_replace(true);
    assert_closed(&mut stream).await;
    drop(blocked);
}

#[tokio::test]
async fn a_shared_host_refuses_socket_upgrades_from_other_origins() {
    let state = AppState::hosted(
        Arc::new(WebSocketEventBus::new()),
        None,
        "test-mac",
        crate::test_support::registry(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(crate::serve_host(listener, state, std::future::pending()));

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let handshake = format!(
        "GET /api/v1/ws HTTP/1.1\r\nHost: {addr}\r\nOrigin: https://attacker.example\r\n\
         Upgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(handshake.as_bytes()).await.unwrap();
    let mut status = [0u8; 12];
    stream.read_exact(&mut status).await.unwrap();
    assert_eq!(&status, b"HTTP/1.1 403");

    upgrade(addr, "/api/v1/ws").await;
}

#[tokio::test]
async fn a_device_token_on_a_server_without_an_api_token_becomes_a_device() {
    let host = crate::test_support::server("ws-open-device").await;
    let core = host.state.core().await.unwrap();
    let (device, token) = core.pair_device("Pixel", "android").await.unwrap();
    let mut stream = connect_state(host.state.clone()).await;

    send_json(&mut stream, json!({ "type": "auth", "token": token })).await;
    let ok = next_message(&mut stream).await;
    assert_eq!(ok["payload"]["deviceId"], device.id);

    send_json(
        &mut stream,
        json!({ "type": "request", "id": 1, "method": "GET", "path": "/devices" }),
    )
    .await;
    let response = next_message(&mut stream).await;
    let body: Value = serde_json::from_str(response["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["devices"][0]["current"], true);

    core.revoke_device(&device.id).await.unwrap();
    let removed = next_message(&mut stream).await;
    assert_eq!(removed["payload"]["code"], "DEVICE_REMOVED");
    assert_closed(&mut stream).await;
}

#[tokio::test]
async fn a_failed_device_lookup_is_not_reported_as_a_revoked_device() {
    let host = hosted("ws-lookup-failure").await;
    let core = host.state.core().await.unwrap();
    let (_, token) = core.pair_device("Pixel", "android").await.unwrap();
    let pipedash_core::infrastructure::database::DatabasePool::Sqlite(pool) =
        core.provider_service.repository().cache_pool().clone()
    else {
        panic!("expected a SQLite pool");
    };
    pool.close().await;
    let mut stream = connect_state(host.state.clone()).await;

    send_json(&mut stream, json!({ "type": "auth", "token": token })).await;

    let rejected = next_message(&mut stream).await;
    assert_eq!(rejected["payload"]["code"], "SERVER_NOT_READY");
    assert_closed(&mut stream).await;
}

#[tokio::test]
async fn a_locked_host_answers_host_locked_instead_of_forgetting_the_device() {
    let state = AppState::hosted(
        Arc::new(WebSocketEventBus::new()),
        None,
        "test-mac",
        crate::test_support::registry(),
    );
    let mut stream = connect_state(state).await;

    send_json(&mut stream, json!({ "type": "auth", "token": "any" })).await;

    let rejected = next_message(&mut stream).await;
    assert_eq!(rejected["payload"]["code"], "HOST_LOCKED");
    assert_eq!(rejected["payload"]["message"], crate::HOST_LOCKED_MESSAGE);
    assert_closed(&mut stream).await;
}

#[tokio::test]
async fn a_server_with_a_broken_config_does_not_forget_its_devices() {
    let state = AppState::config_error(
        Arc::new(WebSocketEventBus::new()),
        "broken".into(),
        crate::test_support::registry(),
    );
    state.set_api_token("vault-password");
    let mut stream = connect_state(state).await;

    send_json(
        &mut stream,
        json!({ "type": "auth", "token": "device-token" }),
    )
    .await;

    let rejected = next_message(&mut stream).await;
    assert_eq!(rejected["payload"]["code"], "SERVER_NOT_READY");
    assert_closed(&mut stream).await;
}

#[tokio::test]
async fn a_pairing_code_can_be_claimed_only_once() {
    let host = hosted("ws-claim-once").await;
    let (code, _) = host.state.pairing_codes.issue().unwrap();

    let mut first = connect_state(host.state.clone()).await;
    assert_eq!(claim(&mut first, &code).await["type"], "auth-ok");

    let mut second = connect_state(host.state.clone()).await;
    let rejected = claim(&mut second, &code).await;
    assert_eq!(rejected["type"], "auth-error");
    assert_eq!(rejected["payload"]["code"], "PAIRING_CODE_INVALID");
    assert_closed(&mut second).await;
}

#[tokio::test]
async fn a_claim_with_invalid_device_details_is_rejected_without_burning_the_code() {
    let host = hosted("ws-claim-invalid").await;
    let (code, _) = host.state.pairing_codes.issue().unwrap();
    let mut stream = connect_state(host.state.clone()).await;

    send_json(
        &mut stream,
        json!({ "type": "claim", "code": code, "name": "Pixel", "platform": "palm" }),
    )
    .await;

    assert_eq!(
        next_message(&mut stream).await["payload"]["code"],
        "INVALID_PLATFORM"
    );
    assert!(host.state.pairing_codes.take(&code));
}

#[tokio::test]
async fn revoking_a_device_closes_its_socket_with_device_removed() {
    let host = hosted("ws-revoke").await;
    let (code, _) = host.state.pairing_codes.issue().unwrap();
    let mut stream = connect_state(host.state.clone()).await;
    let ok = claim(&mut stream, &code).await;
    let device_id = ok["payload"]["deviceId"].as_str().unwrap().to_string();

    host.state
        .core()
        .await
        .unwrap()
        .revoke_device(&device_id)
        .await
        .unwrap();

    let removed = next_message(&mut stream).await;
    assert_eq!(removed["type"], "auth-error");
    assert_eq!(removed["payload"]["code"], "DEVICE_REMOVED");
    assert_closed(&mut stream).await;
}

#[tokio::test]
async fn a_connection_that_never_authenticates_is_closed_after_ten_seconds() {
    let host = hosted("ws-timeout").await;
    let mut stream = connect_state(host.state.clone()).await;

    stream.write_all(&[0x89, 0x80, 0, 0, 0, 0]).await.unwrap();
    let (opcode, _) = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
        .await
        .expect("no pong arrived")
        .expect("the socket closed");
    assert_eq!(opcode, 0xA);

    tokio::time::pause();
    tokio::time::advance(AUTH_TIMEOUT + Duration::from_secs(1)).await;
    tokio::time::resume();

    let timed_out = next_message(&mut stream).await;
    assert_eq!(timed_out["type"], "auth-error");
    assert_eq!(timed_out["payload"]["code"], "AUTH_TIMEOUT");
    assert_closed(&mut stream).await;
}

fn failed_pipeline(name: &str) -> pipedash_core::Pipeline {
    serde_json::from_value(json!({
        "id": format!("stub__1__{name}"),
        "provider_id": 1,
        "provider_type": "stub",
        "name": name,
        "status": "failed",
        "last_run": null,
        "last_updated": chrono::Utc::now(),
        "repository": "",
        "branch": null,
        "workflow_file": null,
    }))
    .unwrap()
}

async fn started(name: &str) -> (crate::test_support::Hosted, CoreContext) {
    let host = crate::test_support::server(name).await;
    let core = host.state.core().await.unwrap();
    core.start_background_tasks().await;

    (host, core)
}

async fn fail(core: &CoreContext, names: &[&str]) {
    core.event_bus
        .emit(CoreEvent::PipelinesFailed {
            pipelines: names.iter().copied().map(failed_pipeline).collect(),
        })
        .await;
}

async fn recorded(core: &CoreContext, count: usize) -> Vec<ObservedNotification> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let seen = core.notifications_after(0);
            if seen.len() >= count {
                return seen;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the recorder never observed the failures")
}

async fn next_notification(stream: &mut TcpStream) -> Value {
    loop {
        let message = next_message(stream).await;
        assert_ne!(message["type"], "notification-observed");
        if message["type"] == "notification" {
            return message["payload"].clone();
        }
    }
}

async fn synced(stream: &mut TcpStream) {
    send_json(
        stream,
        json!({ "type": "request", "id": 99, "method": "GET", "path": "/providers" }),
    )
    .await;
    loop {
        let message = next_message(stream).await;
        if message["type"] == "response" && message["id"] == 99 {
            return;
        }
    }
}

#[tokio::test]
async fn a_browser_that_muted_a_pipeline_gets_no_notification_for_it() {
    let (host, core) = started("ws-notify-muted").await;
    host.state.set_api_token("vault-password");
    let mut stream = connect_state(host.state.clone()).await;
    send_json(
        &mut stream,
        json!({ "type": "auth", "token": "vault-password" }),
    )
    .await;
    assert_eq!(next_message(&mut stream).await["type"], "auth-ok");
    send_json(
        &mut stream,
        json!({
            "type": "notification-preferences",
            "preferences": { "muted_pipelines": ["stub__1__muted"] },
        }),
    )
    .await;
    synced(&mut stream).await;

    fail(&core, &["muted", "other"]).await;

    let shown = next_notification(&mut stream).await;
    assert_eq!(shown["pipeline_id"], "stub__1__other");
    assert_eq!(shown["kind"], "failed");
    assert_eq!(shown["tone"], "danger");
    assert_eq!(shown["fired"], Value::Null);
}

#[tokio::test]
async fn a_reconnecting_browser_gets_the_missed_notifications_once() {
    let (host, core) = started("ws-notify-replay").await;
    fail(&core, &["a", "b"]).await;
    let seen = recorded(&core, 2).await;
    let mut stream = connect_state(host.state.clone()).await;

    send_json(
        &mut stream,
        json!({ "type": "notification-preferences", "preferences": {}, "after": seen[0].id }),
    )
    .await;
    let replayed = next_notification(&mut stream).await;
    assert_eq!(replayed["id"], seen[1].id);
    assert_eq!(replayed["pipeline_id"], "stub__1__b");

    fail(&core, &["c"]).await;
    assert_eq!(
        next_notification(&mut stream).await["pipeline_id"],
        "stub__1__c"
    );

    send_json(
        &mut stream,
        json!({ "type": "notification-preferences", "preferences": {}, "after": 0 }),
    )
    .await;
    synced(&mut stream).await;
    fail(&core, &["d"]).await;
    assert_eq!(
        next_notification(&mut stream).await["pipeline_id"],
        "stub__1__d"
    );
}

#[tokio::test]
async fn a_replay_goes_through_the_preferences_the_browser_sent() {
    let (host, core) = started("ws-notify-replay-muted").await;
    fail(&core, &["muted", "other"]).await;
    recorded(&core, 2).await;
    let mut stream = connect_state(host.state.clone()).await;

    send_json(
        &mut stream,
        json!({
            "type": "notification-preferences",
            "preferences": { "muted_pipelines": ["stub__1__muted"] },
            "after": 0,
        }),
    )
    .await;

    assert_eq!(
        next_notification(&mut stream).await["pipeline_id"],
        "stub__1__other"
    );
}

#[tokio::test]
async fn a_device_connection_uses_the_preferences_stored_for_it() {
    let (host, core) = started("ws-notify-device").await;
    let (device, token) = core.pair_device("Pixel", "android").await.unwrap();
    core.devices()
        .set_notification_preferences(
            &device.id,
            &NotificationPreferences {
                muted_pipelines: vec!["stub__1__muted".into()],
                ..NotificationPreferences::default()
            },
        )
        .await
        .unwrap();
    let mut stream = connect_state(host.state.clone()).await;
    send_json(&mut stream, json!({ "type": "auth", "token": token })).await;
    assert_eq!(next_message(&mut stream).await["type"], "auth-ok");
    send_json(
        &mut stream,
        json!({
            "type": "notification-preferences",
            "preferences": { "muted_pipelines": ["stub__1__other"] },
        }),
    )
    .await;
    synced(&mut stream).await;

    fail(&core, &["muted", "other"]).await;

    assert_eq!(
        next_notification(&mut stream).await["pipeline_id"],
        "stub__1__other"
    );
    let stored = core.devices().notification_preferences().await.unwrap();
    assert_eq!(stored[&device.id].muted_pipelines, ["stub__1__muted"]);
}

#[tokio::test]
async fn the_raw_notification_event_is_never_sent_and_needs_a_core() {
    let bus = Arc::new(WebSocketEventBus::new());
    let mut stream = connect(&bus).await;
    let notice = pipedash_core::application::LocalNotification {
        key: "pipeline:x".into(),
        title: "x failed".into(),
        body: String::new(),
        pipeline_id: Some("x".into()),
        provider_id: Some(1),
        provider_ids: vec![1],
        kind: pipedash_core::domain::AlertKind::Failed,
        finish: None,
        level: None,
        link: None,
    };

    bus.emit(CoreEvent::NotificationObserved {
        notification: ObservedNotification {
            id: 1,
            at: chrono::Utc::now(),
            notice,
        },
    })
    .await;
    bus.emit(CoreEvent::ProvidersChanged).await;

    assert_eq!(next_message(&mut stream).await["type"], "providers-changed");
}

#[tokio::test]
async fn locking_the_vault_tells_connected_clients() {
    let host = hosted("ws-vault-lock").await;
    host.state.set_api_token("vault-password");
    let mut stream = connect_state(host.state.clone()).await;
    send_json(
        &mut stream,
        json!({ "type": "auth", "token": "vault-password" }),
    )
    .await;
    assert_eq!(next_message(&mut stream).await["type"], "auth-ok");

    let response = tower::ServiceExt::oneshot(
        crate::routes::api_router().with_state(host.state.clone()),
        axum::http::Request::post("/vault/lock")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    loop {
        let message = next_message(&mut stream).await;
        if message["type"] == "vault-locked" {
            assert_eq!(message["payload"], json!({}));
            break;
        }
    }
}
