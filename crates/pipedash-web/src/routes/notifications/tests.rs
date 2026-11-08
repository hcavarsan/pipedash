use std::sync::Arc;

use axum::body::{
    to_bytes,
    Body,
};
use axum::http::{
    Request,
    StatusCode,
};
use serde_json::{
    json,
    Value,
};
use tower::ServiceExt;

use crate::state::AppState;
use crate::ws::event_bus::WebSocketEventBus;

const TOKEN: &str = "operator-secret";

fn notice(key: &str, at: i64) -> Value {
    json!({
        "key": key, "at": at, "read": false, "tone": "danger", "level": null,
        "title": key, "body": "", "providerIds": [], "pipelineId": null,
        "link": null, "runNumber": null,
    })
}

async fn post(token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let state = AppState::config_error(
        Arc::new(WebSocketEventBus::new()),
        "test".into(),
        crate::test_support::registry(),
    );
    state.set_api_token(TOKEN);
    let mut request =
        Request::post("/api/v1/notifications/center").header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = crate::app(state, None)
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn the_center_route_needs_the_api_token() {
    let body = json!({
        "snapshot": { "revision": 0, "items": [], "unread": 0 },
        "change": { "type": "clear" },
    });

    assert_eq!(post(None, body.clone()).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(post(Some("wrong"), body).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_center_route_applies_a_change_to_the_posted_snapshot() {
    let (status, body) = post(
        Some(TOKEN),
        json!({
            "snapshot": { "revision": 3, "items": [notice("a", 1)], "unread": 1 },
            "change": { "type": "push", "notice": notice("b", 2) },
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revision"], 4);
    assert_eq!(body["unread"], 2);
    assert_eq!(body["items"][0]["key"], "b");
    assert_eq!(body["items"][1]["key"], "a");
}

#[tokio::test]
async fn the_center_route_rejects_a_snapshot_above_the_item_cap() {
    let items: Vec<_> = (0..51).map(|index| notice(&index.to_string(), 1)).collect();

    let (status, _) = post(
        Some(TOKEN),
        json!({
            "snapshot": { "revision": 0, "items": items, "unread": 51 },
            "change": { "type": "clear" },
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}
