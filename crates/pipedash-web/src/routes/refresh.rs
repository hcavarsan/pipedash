use axum::{
    extract::State,
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::application::RefreshMode;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct RefreshModeResponse {
    pub mode: String,
}

#[derive(Debug, Deserialize)]
pub struct SetRefreshModeRequest {
    pub mode: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/mode", get(get_refresh_mode).put(set_refresh_mode))
        .route("/all", post(refresh_all))
}

async fn get_refresh_mode(State(state): State<AppState>) -> ApiResult<Json<RefreshModeResponse>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let mode = core.refresh_manager.get_mode().await;
    Ok(Json(RefreshModeResponse {
        mode: mode.as_str().to_string(),
    }))
}

async fn set_refresh_mode(
    State(state): State<AppState>, Json(req): Json<SetRefreshModeRequest>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let refresh_mode = req.mode.parse::<RefreshMode>().unwrap_or(RefreshMode::Idle);
    core.refresh_manager.set_mode(refresh_mode).await;
    Ok(())
}

async fn refresh_all(State(state): State<AppState>) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service.clear_all_run_history_caches().await;
    core.pipeline_service.refresh_all().await?;
    Ok(())
}
