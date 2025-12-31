use std::collections::HashMap;

use axum::{
    extract::{
        Path,
        State,
    },
    routing::get,
    Json,
    Router,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SavePreferencesRequest {
    pub preferences_json: String,
}

#[derive(Debug, Serialize)]
pub struct DefaultPreferences {
    #[serde(rename = "columnOrder")]
    pub column_order: Vec<String>,
    #[serde(rename = "columnVisibility")]
    pub column_visibility: HashMap<String, bool>,
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
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let preferences = core
        .provider_service
        .repository()
        .get_table_preferences(provider_id, &table_id)
        .await?;
    Ok(Json(preferences))
}

async fn save_table_preferences(
    State(state): State<AppState>, Path((provider_id, table_id)): Path<(i64, String)>,
    Json(req): Json<SavePreferencesRequest>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.provider_service
        .repository()
        .upsert_table_preferences(provider_id, &table_id, &req.preferences_json)
        .await?;
    Ok(())
}

async fn get_default_table_preferences(
    State(state): State<AppState>, Path((provider_id, table_id)): Path<(i64, String)>,
) -> ApiResult<Json<DefaultPreferences>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let provider_config = core
        .provider_service
        .get_provider_config(provider_id)
        .await?;

    let metadata_list = core.provider_service.list_available_plugins();
    let plugin_metadata = metadata_list
        .iter()
        .find(|m| m.provider_type == provider_config.provider_type)
        .ok_or_else(|| {
            AppError::not_found(format!(
                "Plugin not found for provider type: {}",
                provider_config.provider_type
            ))
        })?;

    let table = plugin_metadata
        .table_schema
        .get_table(&table_id)
        .ok_or_else(|| AppError::not_found(format!("Table '{}' not found in schema", table_id)))?;

    let column_visibility: HashMap<String, bool> = table
        .columns
        .iter()
        .map(|col| (col.id.clone(), col.default_visible))
        .collect();

    let column_order: Vec<String> = table.columns.iter().map(|col| col.id.clone()).collect();

    Ok(Json(DefaultPreferences {
        column_order,
        column_visibility,
    }))
}
