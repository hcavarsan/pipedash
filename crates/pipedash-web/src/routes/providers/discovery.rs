use std::collections::HashMap;

use axum::{
    extract::State,
    Json,
};
use pipedash_core::domain::PaginatedAvailablePipelines;
use pipedash_plugin_api::Organization;
use serde::Deserialize;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct FetchOrganizationsRequest {
    pub provider_type: String,
    pub config: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewPipelinesRequest {
    pub provider_type: String,
    pub config: HashMap<String, String>,
    pub org: Option<String>,
    pub search: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

pub(super) async fn fetch_organizations(
    State(state): State<AppState>, Json(req): Json<FetchOrganizationsRequest>,
) -> ApiResult<Json<Vec<Organization>>> {
    let orgs = state
        .provider_service()
        .await?
        .discover_organizations(&req.provider_type, req.config)
        .await?;

    Ok(Json(orgs))
}

pub(super) async fn preview_pipelines(
    State(state): State<AppState>, Json(req): Json<PreviewPipelinesRequest>,
) -> ApiResult<Json<PaginatedAvailablePipelines>> {
    let result = state
        .provider_service()
        .await?
        .preview_pipelines(
            &req.provider_type,
            req.config,
            req.org,
            req.search,
            req.page,
            req.page_size,
        )
        .await?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct GetFieldOptionsRequest {
    pub provider_type: String,
    pub field_key: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
}

pub(super) async fn get_field_options(
    State(state): State<AppState>, Json(req): Json<GetFieldOptionsRequest>,
) -> ApiResult<Json<Vec<String>>> {
    let provider_service = state.provider_service().await?;
    let options = provider_service
        .field_options(&req.provider_type, &req.field_key, req.config)
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch field options: {e}")))?;

    Ok(Json(options))
}
