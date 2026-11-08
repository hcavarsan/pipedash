use std::collections::HashMap;

use pipedash_core::domain::provider_draft::{
    ProviderDraft,
    ProviderDraftResult,
};
use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn validate_provider_draft(
    maybe_core: State<'_, crate::MaybeCoreContext>, draft: ProviderDraft,
) -> Result<ProviderDraftResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core.provider_service.validate_draft(&draft).await?)
}

#[tauri::command]
pub async fn save_provider_draft(
    maybe_core: State<'_, crate::MaybeCoreContext>, draft: ProviderDraft,
) -> Result<i64, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core.save_provider_draft(draft).await?)
}

#[tauri::command]
pub async fn get_matching_pipeline_ids(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>, organization: Option<String>, search: Option<String>,
) -> Result<Vec<String>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core
        .provider_service
        .matching_pipeline_ids(&provider_type, config, organization, search)
        .await?)
}
