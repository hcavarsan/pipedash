use axum::body::Body;
use axum::http::{
    header::AUTHORIZATION,
    Request,
};
use pipedash_core::domain::{
    AlertKind,
    NewDeviceAlert,
    NotificationPreferences,
};
use serde_json::Value;
use tower::ServiceExt;

use super::*;
use crate::test_support::{
    hosted,
    Hosted,
};

async fn device_app(host: &Hosted) -> (Router, String) {
    let (_, token) = host
        .state
        .core()
        .await
        .unwrap()
        .pair_device("Pixel", "ios")
        .await
        .unwrap();

    (crate::app(host.state.clone(), None), token)
}

async fn get_json(app: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
    let request = Request::get(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn new_alert(kind: AlertKind, key: &str) -> NewDeviceAlert {
    NewDeviceAlert {
        key: key.into(),
        kind,
        title: format!("{key} title"),
        body: format!("{key} body"),
        pipeline_id: Some(key.into()),
        provider_id: Some(7),
        provider_ids: vec![7],
        finish: None,
    }
}

#[tokio::test]
async fn alerts_are_read_after_a_cursor_without_test_rows() {
    let host = hosted("alerts-after").await;
    let alerts = host.state.core().await.unwrap().device_alerts();
    let first = alerts
        .insert(&new_alert(AlertKind::Failed, "pipeline:a"))
        .await
        .unwrap();
    alerts
        .insert(&new_alert(AlertKind::Test, "test:x"))
        .await
        .unwrap();
    let last = alerts
        .insert(&new_alert(AlertKind::Recovered, "pipeline:b"))
        .await
        .unwrap();
    let (app, token) = device_app(&host).await;

    let (status, body) = get_json(&app, "/api/v1/alerts", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["alerts"].as_array().unwrap().len(), 2);
    assert_eq!(body["alerts"][0]["id"], first.id);
    assert_eq!(body["alerts"][0]["kind"], "failed");
    assert_eq!(body["alerts"][0]["tone"], "danger");
    assert_eq!(body["alerts"][1]["tone"], "success");
    assert_eq!(body["alerts"][0]["key"], "pipeline:a");
    assert_eq!(body["alerts"][0]["pipeline_id"], "pipeline:a");
    assert_eq!(body["alerts"][0]["provider_id"], 7);
    assert!(body["alerts"][0]["created_at"].is_string());
    assert_eq!(body["latest_seq"], last.seq);

    let uri = format!("/api/v1/alerts?after={}", first.seq);
    let (_, body) = get_json(&app, &uri, &token).await;
    let listed = body["alerts"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], last.id);

    let (_, body) = get_json(&app, "/api/v1/alerts?limit=0", &token).await;
    assert_eq!(body["alerts"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_device_lists_only_alerts_its_preferences_allow_and_pages_stay_full() {
    let host = hosted("alerts-finished").await;
    let core = host.state.core().await.unwrap();
    let alerts = core.device_alerts();
    let (app, token) = device_app(&host).await;
    let device = core.devices().list().await.unwrap().remove(0);
    core.devices()
        .set_notification_preferences(
            &device.id,
            &NotificationPreferences {
                watched_pipelines: vec!["pipeline:watched".into()],
                muted_pipelines: vec!["pipeline:muted".into()],
                ..NotificationPreferences::default()
            },
        )
        .await
        .unwrap();
    let failed = alerts
        .insert(&new_alert(AlertKind::Failed, "pipeline:a"))
        .await
        .unwrap();
    alerts
        .insert(&new_alert(AlertKind::Failed, "pipeline:muted"))
        .await
        .unwrap();
    for _ in 0..3 {
        alerts
            .insert(&new_alert(AlertKind::Finished, "pipeline:other"))
            .await
            .unwrap();
    }
    let watched = alerts
        .insert(&new_alert(AlertKind::Finished, "pipeline:watched"))
        .await
        .unwrap();
    let later = alerts
        .insert(&new_alert(AlertKind::Failed, "pipeline:b"))
        .await
        .unwrap();

    let ids = |body: &Value| {
        body["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|alert| alert["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let (_, body) = get_json(&app, "/api/v1/alerts?limit=2", &token).await;
    assert_eq!(ids(&body), [failed.id.clone(), watched.id.clone()]);
    assert_eq!(body["latest_seq"], later.seq);

    let uri = format!("/api/v1/alerts?after={}&limit=2", watched.seq);
    let (_, body) = get_json(&app, &uri, &token).await;
    assert_eq!(ids(&body), [later.id]);
}

#[tokio::test]
async fn one_alert_is_fetched_by_id_and_a_missing_one_is_not_found() {
    let host = hosted("alerts-get").await;
    let alerts = host.state.core().await.unwrap().device_alerts();
    let test = alerts
        .insert(&new_alert(AlertKind::Test, "test:x"))
        .await
        .unwrap();
    let (app, token) = device_app(&host).await;

    let (status, body) = get_json(&app, &format!("/api/v1/alerts/{}", test.id), &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["title"], "test:x title");
    assert_eq!(body["kind"], "test");
    assert_eq!(body["tone"], "success");

    let (status, body) = get_json(&app, "/api/v1/alerts/missing", &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "ALERT_NOT_FOUND");
    assert_eq!(body["message"], "This alert is no longer available.");
}
