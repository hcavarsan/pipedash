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
use chrono::Utc;
use pipedash_core::application::services::device_service::DeviceView;
use pipedash_core::domain::{
    change_preferences,
    NotificationPreferenceSnapshot,
    NotificationPreferences,
    OnceWatch,
    PreferenceChange,
};
use pipedash_core::infrastructure::push_relay::RelayOutcome;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ApiError,
    ApiResult,
    AppError,
};
use crate::pairing::{
    self,
    PairingResponse,
};
use crate::state::{
    AppState,
    CurrentDevice,
};

#[derive(Debug, Serialize)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceView>,
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
            get(get_notification_preferences)
                .put(set_notification_preferences)
                .patch(change_notification_preference),
        )
}

async fn list_devices(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
) -> ApiResult<Json<ListDevicesResponse>> {
    let current = current.as_ref().map(|Extension(device)| device.0.as_str());
    let devices = state
        .core()
        .await?
        .list_devices(current, !crate::demo::enabled())
        .await?;

    Ok(Json(ListDevicesResponse { devices }))
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
    let relay_available = state.push_relay_url().await.is_some();
    state
        .core()
        .await?
        .set_device_push(&device_id, &req.subscription_id, relay_available)
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
    let fired = state
        .core()
        .await?
        .set_device_notification_preferences(&device_id, preferences)
        .await?;

    Ok(Json(NotificationPreferencesResponse { fired }))
}

#[derive(Deserialize)]
pub struct ChangePreferenceRequest {
    #[serde(flatten)]
    change: PreferenceChange,
    #[serde(default)]
    preferences: Option<NotificationPreferences>,
}

async fn change_notification_preference(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
    Json(request): Json<ChangePreferenceRequest>,
) -> ApiResult<Json<NotificationPreferenceSnapshot>> {
    let ChangePreferenceRequest {
        change,
        preferences,
    } = request;
    let Some(Extension(CurrentDevice(device_id))) = current else {
        let mut preferences = preferences.ok_or_else(device_only)?;
        change_preferences(&mut preferences, change, Utc::now())?;

        return Ok(Json(NotificationPreferenceSnapshot::new(preferences)));
    };
    let snapshot = state
        .core()
        .await?
        .notification_preferences(&device_id)
        .change(change)
        .await?;

    Ok(Json(snapshot))
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

    match state
        .push_service()
        .await?
        .test_device_push(&device_id)
        .await?
    {
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
mod tests;

#[cfg(test)]
mod undelivered_tests;
