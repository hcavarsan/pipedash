use std::collections::HashMap;

use axum::{
    extract::{
        Path,
        State,
    },
    Json,
};
use pipedash_core::application::services::provider_service::dto::PermissionCheckResult;
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CheckPermissionsRequest {
    pub provider_type: String,
    pub config: HashMap<String, String>,
}

pub(super) async fn get_provider_table_schema(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<pipedash_plugin_api::schema::TableSchema>> {
    let schema = state
        .provider_service()
        .await?
        .get_provider_table_schema(id)
        .await?;

    Ok(Json(schema))
}

pub(super) async fn check_permissions(
    State(state): State<AppState>, Json(req): Json<CheckPermissionsRequest>,
) -> ApiResult<Json<PermissionCheckResult>> {
    let result = state
        .provider_service()
        .await?
        .check_permissions(&req.provider_type, req.config)
        .await?;

    Ok(Json(result))
}
