use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use pipedash_core::application::services::setup_service::SetupService;
use pipedash_core::domain::setup::{
    SetupApplied,
    SetupDraft,
    SetupDraftResult,
};

use super::session::reload_storage;
use crate::error::ApiResult;
use crate::state::AppState;

pub(super) async fn prepare_draft(
    State(state): State<AppState>, Json(draft): Json<SetupDraft>,
) -> ApiResult<Json<SetupDraftResult>> {
    let core = Arc::new(state.core().await?);
    let service = SetupService::new(super::config_path().await?, Some(core), false);

    Ok(Json(service.prepare(&draft).await?))
}

pub(super) async fn apply_draft(
    State(state): State<AppState>, Json(draft): Json<SetupDraft>,
) -> ApiResult<Json<SetupApplied>> {
    let _gate = state.setup_lock.lock().await;
    let core = Arc::new(state.core().await?);
    let service = SetupService::new(super::config_path().await?, Some(core), false);
    let mut applied = service.apply(draft).await?;
    reload_storage(&state).await?;
    applied.restart_required = false;

    Ok(Json(applied))
}
