use axum::body::Body;
use axum::http::{
    header::{
        AUTHORIZATION,
        CONTENT_TYPE,
    },
    Method,
    Request,
};
use serde_json::{
    json,
    Value,
};
use tower::ServiceExt;

use super::*;
use crate::test_support::{
    hosted,
    stub_server,
    Hosted,
    StubRelay,
};
use crate::ws::event_bus::WebSocketEventBus;

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => request
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn pairing_app() -> Router {
    let state = AppState::setup_mode(
        std::sync::Arc::new(WebSocketEventBus::new()),
        crate::test_support::registry(),
    );
    Router::new().nest("/devices", router()).with_state(state)
}

#[tokio::test]
async fn pairing_for_a_loopback_address_returns_a_code_link_and_qr() {
    let app = pairing_app();

    let (status, body) = call(
        &app,
        Method::POST,
        "/devices/pairing",
        Some(json!({ "server_url": "http://localhost:8080/" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["loopback"], true);
    assert_eq!(body["server_url"], "http://localhost:8080");
    let code = body["code"].as_str().unwrap();
    assert_eq!(code.len(), 9);
    assert_eq!(code.as_bytes()[4], b'-');
    assert!(body["link"]
        .as_str()
        .unwrap()
        .starts_with("pipedash://pair?url="));
    assert!(body["qr_svg"].as_str().unwrap().contains("<svg"));
    assert!(body["expires_at"].is_string());
}

#[tokio::test]
async fn pairing_for_an_invalid_address_is_unprocessable() {
    let app = pairing_app();

    let (status, body) = call(
        &app,
        Method::POST,
        "/devices/pairing",
        Some(json!({ "server_url": "ftp://host" })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INVALID_SERVER_URL");
}

#[tokio::test]
async fn a_device_is_listed_then_revoked_once() {
    let host = hosted("devices-route").await;
    let core = host.state.core().await.unwrap();
    let (device, _) = core.pair_device("Pixel", "android").await.unwrap();
    let app = Router::new()
        .nest("/devices", router())
        .with_state(host.state.clone());

    let (status, body) = call(&app, Method::GET, "/devices", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["devices"][0]["id"], device.id);
    assert_eq!(body["devices"][0]["name"], "Pixel");
    assert_eq!(body["devices"][0]["current"], false);
    assert_eq!(body["devices"][0]["push"], false);

    let uri = format!("/devices/{}", device.id);
    assert_eq!(
        call(&app, Method::DELETE, &uri, None).await.0,
        StatusCode::NO_CONTENT
    );
    let (status, body) = call(&app, Method::DELETE, &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "DEVICE_NOT_FOUND");
}

struct Paired {
    _host: Hosted,
    relay: StubRelay,
    app: Router,
    device_id: String,
    token: String,
}

async fn paired(name: &str, with_relay: bool) -> Paired {
    let host = hosted(name).await;
    let relay = stub_server().await;
    let state = if with_relay {
        host.state.clone().with_push_relay(&relay.url)
    } else {
        host.state.clone()
    };
    state.set_api_token("operator-token");
    let (device, token) = state
        .core()
        .await
        .unwrap()
        .pair_device("Pixel", "ios")
        .await
        .unwrap();

    Paired {
        _host: host,
        relay,
        app: crate::app(state, None),
        device_id: device.id,
        token,
    }
}

async fn call_as(
    app: &Router, method: Method, uri: &str, token: &str, body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(body) => request
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

const PUSH: &str = "/api/v1/devices/current/push";
const PUSH_TEST: &str = "/api/v1/devices/current/push/test";
const PUSH_ACK: &str = "/api/v1/devices/current/push/ack";
const PREFERENCES: &str = "/api/v1/devices/current/notification-preferences";

#[tokio::test]
async fn only_a_paired_device_manages_its_push_registration() {
    let p = paired("push-device-only", true).await;
    let body = json!({ "subscription_id": "stub-sub", "through": 1 });

    for (method, uri) in [
        (Method::GET, PUSH),
        (Method::PUT, PUSH),
        (Method::DELETE, PUSH),
        (Method::POST, PUSH_TEST),
        (Method::POST, PUSH_ACK),
        (Method::GET, PREFERENCES),
        (Method::PUT, PREFERENCES),
    ] {
        let (status, response) = call_as(
            &p.app,
            method.clone(),
            uri,
            "operator-token",
            Some(body.clone()),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert_eq!(response["code"], "DEVICE_ONLY");
        assert_eq!(response["message"], "Only a paired device can do this.");
    }
}

#[tokio::test]
async fn a_device_saves_its_notification_preferences() {
    let p = paired("device-preferences", false).await;

    let (status, body) = call_as(
        &p.app,
        Method::PUT,
        PREFERENCES,
        &p.token,
        Some(json!({ "outage": false, "muted_pipelines": ["stub__1__api"] })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "fired": [] }));
    let saved = p._host.state.core().await.unwrap().devices();
    assert_eq!(
        saved.notification_preferences().await.unwrap()[&p.device_id],
        NotificationPreferences {
            outage: false,
            muted_pipelines: vec!["stub__1__api".into()],
            ..NotificationPreferences::default()
        }
    );
}

#[tokio::test]
async fn a_device_changes_one_preference_and_gets_the_snapshot_back() {
    let p = paired("device-preference-change", false).await;
    let change = json!({ "type": "cancel_next_finish", "pipeline_id": "stub__1__api" });
    let (status, body) = call_as(
        &p.app,
        Method::PATCH,
        PREFERENCES,
        "operator-token",
        Some(change),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "DEVICE_ONLY");

    let (status, body) = call_as(
        &p.app,
        Method::PATCH,
        PREFERENCES,
        &p.token,
        Some(json!({ "type": "pipeline_watched", "pipeline_id": "stub__1__api", "watched": true })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["preferences"]["watched_pipelines"],
        json!(["stub__1__api"])
    );
    assert_eq!(
        body["pipeline_states"],
        json!({ "stub__1__api": "watching" })
    );

    let (status, body) = call_as(
        &p.app,
        Method::PATCH,
        PREFERENCES,
        &p.token,
        Some(json!({ "type": "kind", "kind": "finished", "enabled": false })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn a_device_reads_back_its_stored_preferences_or_null() {
    let p = paired("device-preferences-read", false).await;

    let (status, body) = call_as(&p.app, Method::GET, PREFERENCES, &p.token, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "preferences": null }));

    call_as(
        &p.app,
        Method::PUT,
        PREFERENCES,
        &p.token,
        Some(json!({ "outage": false, "muted_pipelines": ["stub__1__api"] })),
    )
    .await;

    let (status, body) = call_as(&p.app, Method::GET, PREFERENCES, &p.token, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["preferences"],
        serde_json::to_value(NotificationPreferences {
            outage: false,
            muted_pipelines: vec!["stub__1__api".into()],
            ..NotificationPreferences::default()
        })
        .unwrap()
    );
}

#[tokio::test]
async fn resending_preferences_keeps_a_used_one_time_watch_used() {
    let p = paired("device-preferences-once", false).await;
    let request = json!({ "pipeline_id": "stub__1__api", "since": "2026-11-04T10:00:00Z" });
    let devices = p._host.state.core().await.unwrap().devices();
    let saved = || async {
        devices.notification_preferences().await.unwrap()[&p.device_id]
            .watch_once
            .clone()
    };

    call_as(
        &p.app,
        Method::PUT,
        PREFERENCES,
        &p.token,
        Some(json!({ "watch_once": [request] })),
    )
    .await;
    let mut used = devices.notification_preferences().await.unwrap()[&p.device_id].clone();
    let fired_at = chrono::Utc::now();
    assert_eq!(
        used.fire_once(
            "stub__1__api",
            &pipedash_core::domain::RunFinish {
                at: fired_at,
                previous: None,
            }
        )
        .len(),
        1
    );
    devices
        .set_notification_preferences(&p.device_id, &used)
        .await
        .unwrap();

    let (status, body) = call_as(
        &p.app,
        Method::PUT,
        PREFERENCES,
        &p.token,
        Some(json!({ "watch_once": [request] })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "fired": [{
            "pipeline_id": "stub__1__api",
            "since": "2026-11-04T10:00:00Z",
            "fired_at": fired_at,
        }] })
    );
    assert_eq!(saved().await[0].fired_at, Some(fired_at));
}

#[tokio::test]
async fn a_host_without_a_relay_refuses_push_registration() {
    let p = paired("push-no-relay", false).await;

    let (status, body) = call_as(&p.app, Method::GET, PUSH, &p.token, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({ "relay_url": null, "enabled": false, "undelivered_after": null })
    );

    for (method, uri, request) in [
        (
            Method::PUT,
            PUSH,
            Some(json!({ "subscription_id": "stub-sub" })),
        ),
        (Method::POST, PUSH_TEST, None),
    ] {
        let (status, body) = call_as(&p.app, method, uri, &p.token, request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        assert_eq!(body["code"], "PUSH_RELAY_NOT_CONFIGURED");
        assert_eq!(body["message"], "This host doesn't send push alerts.");
    }
}

#[tokio::test]
async fn a_device_registers_tests_and_clears_its_push() {
    let p = paired("push-flow", true).await;

    let (status, body) = call_as(
        &p.app,
        Method::PUT,
        PUSH,
        &p.token,
        Some(json!({ "subscription_id": "bad id!" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INVALID_SUBSCRIPTION");

    let (status, body) = call_as(&p.app, Method::POST, PUSH_TEST, &p.token, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "PUSH_NOT_REGISTERED");
    assert_eq!(
        body["message"],
        "Turn on notifications on this device first."
    );

    let (status, _) = call_as(
        &p.app,
        Method::PUT,
        PUSH,
        &p.token,
        Some(json!({ "subscription_id": "stub-sub" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = call_as(&p.app, Method::GET, PUSH, &p.token, None).await;
    assert_eq!(
        body,
        json!({ "relay_url": p.relay.url, "enabled": true, "undelivered_after": null })
    );
    let (_, body) = call_as(&p.app, Method::GET, "/api/v1/devices", &p.token, None).await;
    assert_eq!(body["devices"][0]["id"], p.device_id);
    assert_eq!(body["devices"][0]["push"], true);

    let (status, _) = call_as(&p.app, Method::POST, PUSH_TEST, &p.token, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let requests = p.relay.requests();
    assert_eq!(requests.len(), 1);
    let alert_id = requests[0]["alert_id"].as_str().unwrap();
    assert_eq!(
        requests[0],
        json!({ "subscription_id": "stub-sub", "alert_id": alert_id })
    );
    assert_eq!(alert_id.len(), 32);
    assert!(alert_id.bytes().all(|b| b.is_ascii_hexdigit()));

    let (status, alert) = call_as(
        &p.app,
        Method::GET,
        &format!("/api/v1/alerts/{alert_id}"),
        &p.token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{alert}");
    assert_eq!(alert["kind"], "test");
    assert_eq!(alert["title"], "Pipedash test alert");
    assert_eq!(alert["body"], "Push alerts from this host work.");
    assert_eq!(alert["key"], format!("test:{}", p.device_id));
    let (_, listed) = call_as(&p.app, Method::GET, "/api/v1/alerts", &p.token, None).await;
    assert!(listed["alerts"].as_array().unwrap().is_empty());

    for _ in 0..2 {
        let (status, _) = call_as(&p.app, Method::DELETE, PUSH, &p.token, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (_, body) = call_as(&p.app, Method::GET, PUSH, &p.token, None).await;
    assert_eq!(body["enabled"], false);
}

#[tokio::test]
async fn relay_answers_to_a_test_alert_map_to_statuses_and_gone_clears_the_registration() {
    let p = paired("push-statuses", true).await;
    let (status, _) = call_as(
        &p.app,
        Method::PUT,
        PUSH,
        &p.token,
        Some(json!({ "subscription_id": "stub-sub" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    for (relay_status, expected, code) in [
        (429, StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
        (500, StatusCode::BAD_GATEWAY, "PUSH_RELAY_ERROR"),
    ] {
        p.relay.reply_with(relay_status);
        let (status, body) = call_as(&p.app, Method::POST, PUSH_TEST, &p.token, None).await;

        assert_eq!(status, expected);
        assert_eq!(body["code"], code);
        let (_, body) = call_as(&p.app, Method::GET, PUSH, &p.token, None).await;
        assert_eq!(body["enabled"], true);
    }

    p.relay.reply_with(410);
    let (status, body) = call_as(&p.app, Method::POST, PUSH_TEST, &p.token, None).await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(body["code"], "PUSH_SUBSCRIPTION_GONE");
    assert_eq!(
        body["message"],
        "This device's push registration expired. Turn notifications off and on again."
    );
    let (_, body) = call_as(&p.app, Method::GET, PUSH, &p.token, None).await;
    assert_eq!(body["enabled"], false);
}
