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
    Json,
    Router,
};
use chrono::{
    DateTime,
    Utc,
};
use pipedash_core::application::services::push_service::RelaySettings;
use pipedash_core::domain::PushSubscription;
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
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    pub id: String,
    pub device_name: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_success_at: Option<DateTime<Utc>>,
}

impl From<PushSubscription> for SubscriptionResponse {
    fn from(subscription: PushSubscription) -> Self {
        Self {
            id: subscription.id,
            device_name: subscription.device_name,
            platform: subscription.platform,
            created_at: subscription.created_at,
            last_success_at: subscription.last_success_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListSubscriptionsResponse {
    pub relay_configured: bool,
    pub subscriptions: Vec<SubscriptionResponse>,
}

#[derive(Deserialize)]
pub struct SetRelayRequest {
    pub enabled: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/subscriptions", get(list_subscriptions))
        .route("/subscriptions/{id}", delete(delete_subscription))
        .route("/subscriptions/{id}/test", post(test_subscription))
        .route("/relay", get(get_relay).put(set_relay))
}

async fn get_relay(State(state): State<AppState>) -> ApiResult<Json<RelaySettings>> {
    Ok(Json(state.push_service().await?.relay_settings().await?))
}

async fn set_relay(
    State(state): State<AppState>, Json(req): Json<SetRelayRequest>,
) -> ApiResult<Json<RelaySettings>> {
    Ok(Json(
        state
            .push_service()
            .await?
            .set_relay_enabled(req.enabled)
            .await?,
    ))
}

async fn list_subscriptions(
    State(state): State<AppState>,
) -> ApiResult<Json<ListSubscriptionsResponse>> {
    let service = state.push_service().await?;
    let subscriptions = service.list_subscriptions().await?;

    Ok(Json(ListSubscriptionsResponse {
        relay_configured: service.relay_url().await.is_some(),
        subscriptions: subscriptions.into_iter().map(Into::into).collect(),
    }))
}

async fn delete_subscription(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.push_service().await?.delete_subscription(&id).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn test_subscription(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    match state.push_service().await?.test_subscription(&id).await? {
        RelayOutcome::Delivered => Ok(StatusCode::ACCEPTED),
        RelayOutcome::Gone => Err(AppError::not_found(
            "push subscription is no longer registered with the relay",
        )),
        RelayOutcome::RateLimited => Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            ApiError::new("RATE_LIMITED", "push relay rate limit exceeded"),
        )),
        RelayOutcome::Failed => Err(AppError::new(
            StatusCode::BAD_GATEWAY,
            ApiError::new(
                "PUSH_RELAY_ERROR",
                "push relay did not accept the notification",
            ),
        )),
    }
}

#[cfg(test)]
mod tests;
