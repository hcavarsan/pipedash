use axum::{
    routing::post,
    Json,
    Router,
};
use pipedash_core::domain::notification_center::{
    self,
    NotificationCenterChange,
    NotificationCenterSnapshot,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Deserialize)]
struct UpdateCenterRequest {
    snapshot: NotificationCenterSnapshot,
    change: NotificationCenterChange,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/center", post(update_center))
}

async fn update_center(
    Json(request): Json<UpdateCenterRequest>,
) -> ApiResult<Json<NotificationCenterSnapshot>> {
    Ok(Json(notification_center::update_notification_center(
        request.snapshot,
        request.change,
    )?))
}

#[cfg(test)]
mod tests;
