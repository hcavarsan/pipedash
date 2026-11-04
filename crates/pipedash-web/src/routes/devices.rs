use std::collections::HashSet;

use axum::extract::{
    Path,
    State,
};
use axum::http::StatusCode;
use axum::routing::{
    delete,
    get,
    post,
};
use axum::{
    Extension,
    Json,
    Router,
};
use pipedash_core::domain::{
    AlertKind,
    NewDeviceAlert,
    NotificationPreferences,
    OnceWatch,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::push::is_relay_subscription_id;
use crate::error::{
    ApiError,
    ApiResult,
    AppError,
};
use crate::pairing::{
    self,
    DeviceResponse,
    PairingResponse,
};
use crate::push::{
    self,
    RelayOutcome,
};
use crate::state::{
    AppState,
    CurrentDevice,
};

#[derive(Debug, Serialize)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceResponse>,
}

#[derive(Deserialize)]
pub struct CreatePairingRequest {
    pub server_url: String,
}

#[derive(Debug, Serialize)]
pub struct DevicePushResponse {
    pub relay_url: Option<String>,
    pub enabled: bool,
    pub undelivered_after: Option<i64>,
}

#[derive(Deserialize)]
pub struct SetDevicePushRequest {
    pub subscription_id: String,
}

#[derive(Deserialize)]
pub struct AckDevicePushRequest {
    pub through: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationPreferencesResponse {
    pub fired: Vec<OnceWatch>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredNotificationPreferencesResponse {
    pub preferences: Option<NotificationPreferences>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_devices))
        .route("/pairing", post(create_pairing))
        .route("/{id}", delete(revoke_device))
        .route(
            "/current/push",
            get(get_device_push)
                .put(set_device_push)
                .delete(clear_device_push),
        )
        .route("/current/push/test", post(test_device_push))
        .route("/current/push/ack", post(ack_device_push))
        .route(
            "/current/notification-preferences",
            get(get_notification_preferences).put(set_notification_preferences),
        )
}

async fn list_devices(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
) -> ApiResult<Json<ListDevicesResponse>> {
    let core = state.core().await?;
    let devices = core.devices().list().await?;
    let pushes: HashSet<String> = core
        .device_pushes()
        .list()
        .await?
        .into_iter()
        .map(|push| push.device_id)
        .collect();
    let current = current.as_ref().map(|Extension(device)| device.0.as_str());

    Ok(Json(ListDevicesResponse {
        devices: devices
            .into_iter()
            .map(|device| {
                let push = pushes.contains(&device.id);
                pairing::device_response(device, current, push)
            })
            .collect(),
    }))
}

async fn create_pairing(
    State(state): State<AppState>, Json(req): Json<CreatePairingRequest>,
) -> ApiResult<Json<PairingResponse>> {
    pairing::create_pairing(&state, &req.server_url).map(Json)
}

async fn revoke_device(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if state.core().await?.revoke_device(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::new(
            StatusCode::NOT_FOUND,
            ApiError::new("DEVICE_NOT_FOUND", "This device is no longer paired."),
        ))
    }
}

fn device_only() -> AppError {
    AppError::new(
        StatusCode::FORBIDDEN,
        ApiError::new("DEVICE_ONLY", "Only a paired device can do this."),
    )
}

fn push_relay_not_configured() -> AppError {
    AppError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ApiError::new(
            "PUSH_RELAY_NOT_CONFIGURED",
            "This host doesn't send push alerts.",
        ),
    )
}

async fn get_device_push(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
) -> ApiResult<Json<DevicePushResponse>> {
    let Extension(CurrentDevice(device_id)) = current.ok_or_else(device_only)?;
    let push = state.core().await?.device_pushes().get(&device_id).await?;

    Ok(Json(DevicePushResponse {
        relay_url: state.push_relay_url().await,
        enabled: push.is_some(),
        undelivered_after: push
            .and_then(|push| push.undelivered_seq)
            .map(|seq| seq - 1),
    }))
}

async fn set_device_push(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
    Json(req): Json<SetDevicePushRequest>,
) -> ApiResult<StatusCode> {
    let Extension(CurrentDevice(device_id)) = current.ok_or_else(device_only)?;
    if state.push_relay_url().await.is_none() {
        return Err(push_relay_not_configured());
    }
    if !is_relay_subscription_id(&req.subscription_id) {
        return Err(AppError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::new(
                "INVALID_SUBSCRIPTION",
                "This device's push registration isn't valid.",
            ),
        ));
    }
    state
        .core()
        .await?
        .device_pushes()
        .set(&device_id, &req.subscription_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn clear_device_push(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
) -> ApiResult<StatusCode> {
    let Extension(CurrentDevice(device_id)) = current.ok_or_else(device_only)?;
    state
        .core()
        .await?
        .device_pushes()
        .delete(&device_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_notification_preferences(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
) -> ApiResult<Json<StoredNotificationPreferencesResponse>> {
    let Extension(CurrentDevice(device_id)) = current.ok_or_else(device_only)?;
    let preferences = state
        .core()
        .await?
        .devices()
        .notification_preferences()
        .await?
        .remove(&device_id);

    Ok(Json(StoredNotificationPreferencesResponse { preferences }))
}

async fn set_notification_preferences(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
    Json(preferences): Json<NotificationPreferences>,
) -> ApiResult<Json<NotificationPreferencesResponse>> {
    let Extension(CurrentDevice(device_id)) = current.ok_or_else(device_only)?;
    let saved = state
        .core()
        .await?
        .devices()
        .update_notification_preferences(&device_id, |stored| {
            let mut next = preferences.clone();
            next.keep_fired(stored);
            *stored = next;
            true
        })
        .await?
        .unwrap_or_default();

    Ok(Json(NotificationPreferencesResponse {
        fired: saved.fired_watches(),
    }))
}

async fn ack_device_push(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
    Json(req): Json<AckDevicePushRequest>,
) -> ApiResult<StatusCode> {
    let Extension(CurrentDevice(device_id)) = current.ok_or_else(device_only)?;
    state
        .core()
        .await?
        .device_pushes()
        .ack_undelivered(&device_id, req.through)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn test_device_push(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
) -> ApiResult<StatusCode> {
    let Extension(CurrentDevice(device_id)) = current.ok_or_else(device_only)?;
    let relay_url = state
        .push_relay_url()
        .await
        .ok_or_else(push_relay_not_configured)?;
    let core = state.core().await?;
    let push = core.device_pushes().get(&device_id).await?.ok_or_else(|| {
        AppError::new(
            StatusCode::CONFLICT,
            ApiError::new(
                "PUSH_NOT_REGISTERED",
                "Turn on notifications on this device first.",
            ),
        )
    })?;
    let alert = core
        .device_alerts()
        .insert(&NewDeviceAlert {
            key: format!("test:{device_id}"),
            kind: AlertKind::Test,
            title: "Pipedash test alert".into(),
            body: "Push alerts from this host work.".into(),
            pipeline_id: None,
            provider_id: None,
            provider_ids: Vec::new(),
            finish: None,
        })
        .await?;

    match push::deliver_alert(&core, &relay_url, &push, &alert.id, None).await {
        RelayOutcome::Delivered => Ok(StatusCode::ACCEPTED),
        RelayOutcome::Gone => Err(AppError::new(
            StatusCode::GONE,
            ApiError::new(
                "PUSH_SUBSCRIPTION_GONE",
                "This device's push registration expired. Turn notifications off and on again.",
            ),
        )),
        RelayOutcome::RateLimited => Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            ApiError::new("RATE_LIMITED", "The push relay is rate limiting this host."),
        )),
        RelayOutcome::Failed => Err(AppError::new(
            StatusCode::BAD_GATEWAY,
            ApiError::new(
                "PUSH_RELAY_ERROR",
                "The push relay didn't accept the alert.",
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
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
    use crate::ws::WebSocketEventBus;

    async fn call(
        app: &Router, method: Method, uri: &str, body: Option<Value>,
    ) -> (StatusCode, Value) {
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
        let state = AppState::setup_mode(std::sync::Arc::new(WebSocketEventBus::new()));
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
            Some(json!({ "outage": false, "muted_pipelines": ["github__1__api"] })),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, json!({ "fired": [] }));
        let saved = p._host.state.core().await.unwrap().devices();
        assert_eq!(
            saved.notification_preferences().await.unwrap()[&p.device_id],
            NotificationPreferences {
                outage: false,
                muted_pipelines: vec!["github__1__api".into()],
                ..NotificationPreferences::default()
            }
        );
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
            Some(json!({ "outage": false, "muted_pipelines": ["github__1__api"] })),
        )
        .await;

        let (status, body) = call_as(&p.app, Method::GET, PREFERENCES, &p.token, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["preferences"],
            serde_json::to_value(NotificationPreferences {
                outage: false,
                muted_pipelines: vec!["github__1__api".into()],
                ..NotificationPreferences::default()
            })
            .unwrap()
        );
    }

    #[tokio::test]
    async fn resending_preferences_keeps_a_used_one_time_watch_used() {
        let p = paired("device-preferences-once", false).await;
        let request = json!({ "pipeline_id": "github__1__api", "since": "2026-11-04T10:00:00Z" });
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
                "github__1__api",
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
                "pipeline_id": "github__1__api",
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
}

#[cfg(test)]
mod undelivered_tests {
    use axum::body::Body;
    use axum::http::{
        header::{
            AUTHORIZATION,
            CONTENT_TYPE,
        },
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
    };

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

        push::deliver_alert(&core, &relay.url, &push, "a6", Some(6)).await;
        assert_eq!(undelivered_after(&app, &token).await, Value::Null);

        relay.reply_with(503);
        push::deliver_alert(&core, &relay.url, &push, "a7", Some(7)).await;
        assert_eq!(undelivered_after(&app, &token).await, json!(6));

        assert_eq!(ack(&app, &token, 6).await, StatusCode::NO_CONTENT);
        assert_eq!(undelivered_after(&app, &token).await, json!(6));
        assert_eq!(ack(&app, &token, 7).await, StatusCode::NO_CONTENT);
        assert_eq!(undelivered_after(&app, &token).await, Value::Null);

        relay.reply_with(429);
        push::deliver_alert(&core, &relay.url, &push, "a8", Some(8)).await;
        assert_eq!(undelivered_after(&app, &token).await, json!(7));

        relay.reply_with(202);
        push::deliver_alert(&core, &relay.url, &push, "a9", Some(9)).await;
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
            push::deliver_alert(&core, &relay.url, &push, &format!("a{seq}"), Some(seq)).await;
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
}
