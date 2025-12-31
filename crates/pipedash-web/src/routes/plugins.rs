use axum::{
    extract::State,
    routing::get,
    Json,
    Router,
};
use pipedash_plugin_api::PluginMetadata;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_plugins))
        .route("/metadata", get(list_plugin_metadata))
}

async fn list_plugins(State(state): State<AppState>) -> ApiResult<Json<Vec<PluginMetadata>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metadata = core.provider_service.list_available_plugins();
    Ok(Json(metadata))
}

async fn list_plugin_metadata(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<PluginMetadata>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metadata = core.provider_service.list_available_plugins();
    Ok(Json(metadata))
}
