use axum::body::Body;
use axum::http::{
    header::{
        AUTHORIZATION,
        CONTENT_TYPE,
    },
    Request,
};
use pipedash_core::infrastructure::push_relay::RelayOutcome;
use serde_json::{
    json,
    Value,
};
use tower::ServiceExt;

use super::*;
use crate::test_support::{
    hosted,
    stub_server,
};

async fn deliver_alert(
    core: &pipedash_core::CoreContext, relay_url: &str, push: &pipedash_core::domain::DevicePush,
    alert_id: &str, seq: Option<i64>,
) -> RelayOutcome {
    core.push_service()
        .with_fixed_relay(Some(relay_url))
        .deliver_alert(push, alert_id, seq)
        .await
        .unwrap()
}

async fn undelivered_after(app: &Router, token: &str) -> Value {
    let request = Request::get("/api/v1/devices/current/push")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    serde_json::from_slice::<Value>(&body).unwrap()["undelivered_after"].clone()
}

async fn ack(app: &Router, token: &str, through: i64) -> StatusCode {
    let request = Request::post("/api/v1/devices/current/push/ack")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "through": through }).to_string()))
        .unwrap();

    app.clone().oneshot(request).await.unwrap().status()
}

#[tokio::test]
async fn a_failed_push_is_reported_until_the_device_acknowledges_it() {
    let host = hosted("push-undelivered").await;
    let relay = stub_server().await;
    let state = host.state.clone().with_push_relay(&relay.url);
    let core = state.core().await.unwrap();
    let (device, token) = core.pair_device("Pixel", "ios").await.unwrap();
    core.device_pushes().set(&device.id, "sub-a").await.unwrap();
    let push = core.device_pushes().get(&device.id).await.unwrap().unwrap();
    let app = crate::app(state, None);

    deliver_alert(&core, &relay.url, &push, "a6", Some(6)).await;
    assert_eq!(undelivered_after(&app, &token).await, Value::Null);

    relay.reply_with(503);
    deliver_alert(&core, &relay.url, &push, "a7", Some(7)).await;
    assert_eq!(undelivered_after(&app, &token).await, json!(6));

    assert_eq!(ack(&app, &token, 6).await, StatusCode::NO_CONTENT);
    assert_eq!(undelivered_after(&app, &token).await, json!(6));
    assert_eq!(ack(&app, &token, 7).await, StatusCode::NO_CONTENT);
    assert_eq!(undelivered_after(&app, &token).await, Value::Null);

    relay.reply_with(429);
    deliver_alert(&core, &relay.url, &push, "a8", Some(8)).await;
    assert_eq!(undelivered_after(&app, &token).await, json!(7));

    relay.reply_with(202);
    deliver_alert(&core, &relay.url, &push, "a9", Some(9)).await;
    assert_eq!(undelivered_after(&app, &token).await, json!(7));
}

#[tokio::test]
async fn a_partial_ack_keeps_the_rest_of_the_failed_pushes_pending() {
    let host = hosted("push-undelivered-partial").await;
    let relay = stub_server().await;
    relay.reply_with(503);
    let state = host.state.clone().with_push_relay(&relay.url);
    let core = state.core().await.unwrap();
    let (device, token) = core.pair_device("Pixel", "ios").await.unwrap();
    core.device_pushes().set(&device.id, "sub-a").await.unwrap();
    let push = core.device_pushes().get(&device.id).await.unwrap().unwrap();
    let app = crate::app(state, None);

    for seq in 1..=25 {
        deliver_alert(&core, &relay.url, &push, &format!("a{seq}"), Some(seq)).await;
    }
    assert_eq!(undelivered_after(&app, &token).await, json!(0));

    assert_eq!(ack(&app, &token, 20).await, StatusCode::NO_CONTENT);
    assert_eq!(undelivered_after(&app, &token).await, json!(20));

    assert_eq!(ack(&app, &token, 25).await, StatusCode::NO_CONTENT);
    assert_eq!(undelivered_after(&app, &token).await, Value::Null);
}

#[tokio::test]
async fn a_failed_test_alert_is_not_reported_as_undelivered() {
    let host = hosted("push-undelivered-test").await;
    let relay = stub_server().await;
    relay.reply_with(503);
    let state = host.state.clone().with_push_relay(&relay.url);
    let core = state.core().await.unwrap();
    let (device, token) = core.pair_device("Pixel", "ios").await.unwrap();
    core.device_pushes().set(&device.id, "sub-a").await.unwrap();
    let app = crate::app(state, None);

    let request = Request::post("/api/v1/devices/current/push/test")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let status = app.clone().oneshot(request).await.unwrap().status();

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(undelivered_after(&app, &token).await, Value::Null);
}
