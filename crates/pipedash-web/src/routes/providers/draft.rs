use std::collections::HashMap;

use axum::{
    extract::State,
    Json,
};
use pipedash_core::domain::provider_draft::{
    ProviderDraft,
    ProviderDraftResult,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct MatchingIdsRequest {
    pub provider_type: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
    pub organization: Option<String>,
    pub search: Option<String>,
}

pub(super) async fn validate_draft(
    State(state): State<AppState>, Json(draft): Json<ProviderDraft>,
) -> ApiResult<Json<ProviderDraftResult>> {
    let result = state
        .provider_service()
        .await?
        .validate_draft(&draft)
        .await?;

    Ok(Json(result))
}

pub(super) async fn save_draft(
    State(state): State<AppState>, Json(draft): Json<ProviderDraft>,
) -> ApiResult<Json<i64>> {
    let id = state.core().await?.save_provider_draft(draft).await?;

    Ok(Json(id))
}

pub(super) async fn matching_pipeline_ids(
    State(state): State<AppState>, Json(req): Json<MatchingIdsRequest>,
) -> ApiResult<Json<Vec<String>>> {
    let ids = state
        .provider_service()
        .await?
        .matching_pipeline_ids(&req.provider_type, req.config, req.organization, req.search)
        .await?;

    Ok(Json(ids))
}
