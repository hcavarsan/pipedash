use axum::{
    extract::State,
    routing::get,
    Json,
    Router,
};
use pipedash_plugin_api::PluginMetadata;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_plugins))
}

async fn list_plugins(State(state): State<AppState>) -> ApiResult<Json<Vec<PluginMetadata>>> {
    let core = state.core().await?;
    let metadata = core.provider_service.list_available_plugins();
    Ok(Json(metadata))
}
