use axum::extract::{
    Path,
    Query,
    State,
};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{
    Extension,
    Json,
    Router,
};
use pipedash_core::application::services::device_service::alerts::{
    AlertPage,
    AlertView,
};
use serde::Deserialize;

use crate::error::{
    ApiError,
    ApiResult,
    AppError,
};
use crate::state::{
    AppState,
    CurrentDevice,
};

#[derive(Deserialize)]
struct AlertsQuery {
    after: Option<i64>,
    limit: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_alerts))
        .route("/{id}", get(get_alert))
}

async fn list_alerts(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
    Query(query): Query<AlertsQuery>,
) -> ApiResult<Json<AlertPage>> {
    let device_id = current.as_ref().map(|Extension(device)| device.0.as_str());

    Ok(Json(
        state
            .core()
            .await?
            .alert_page(device_id, query.after, query.limit)
            .await?,
    ))
}

async fn get_alert(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<Json<AlertView>> {
    state
        .core()
        .await?
        .device_alerts()
        .get(&id)
        .await?
        .map(|alert| Json(alert.into()))
        .ok_or_else(|| {
            AppError::new(
                StatusCode::NOT_FOUND,
                ApiError::new("ALERT_NOT_FOUND", "This alert is no longer available."),
            )
        })
}

#[cfg(test)]
mod tests;
