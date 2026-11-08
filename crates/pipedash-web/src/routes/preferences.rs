use axum::{
    extract::{
        Path,
        State,
    },
    routing::get,
    Json,
    Router,
};
use pipedash_core::application::services::provider_service::dto::DefaultPreferences;
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SavePreferencesRequest {
    pub preferences_json: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/table/{provider_id}/{table_id}",
            get(get_table_preferences).put(save_table_preferences),
        )
        .route(
            "/table/{provider_id}/{table_id}/default",
            get(get_default_table_preferences),
        )
}

async fn get_table_preferences(
    State(state): State<AppState>, Path((provider_id, table_id)): Path<(i64, String)>,
) -> ApiResult<Json<Option<String>>> {
    let preferences = state
        .provider_service()
        .await?
        .table_preferences(provider_id, &table_id)
        .await?;
    Ok(Json(preferences))
}

async fn save_table_preferences(
    State(state): State<AppState>, Path((provider_id, table_id)): Path<(i64, String)>,
    Json(req): Json<SavePreferencesRequest>,
) -> ApiResult<()> {
    state
        .provider_service()
        .await?
        .save_table_preferences(provider_id, &table_id, &req.preferences_json)
        .await?;
    Ok(())
}

async fn get_default_table_preferences(
    State(state): State<AppState>, Path((provider_id, table_id)): Path<(i64, String)>,
) -> ApiResult<Json<DefaultPreferences>> {
    let preferences = state
        .provider_service()
        .await?
        .default_table_preferences(provider_id, &table_id)
        .await?;
    Ok(Json(preferences))
}
