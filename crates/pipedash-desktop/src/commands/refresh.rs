use pipedash_core::{
    application::RefreshMode,
    domain::RefreshResult,
};
use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn refresh_all(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<Vec<RefreshResult>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.pipeline_service
        .refresh_all()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64,
) -> Result<RefreshResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core.pipeline_service.refresh_provider(id).await)
}

#[tauri::command]
pub async fn set_refresh_mode(
    maybe_core: State<'_, crate::MaybeCoreContext>, mode: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    let refresh_mode = mode.parse::<RefreshMode>().unwrap_or(RefreshMode::Idle);
    core.refresh_manager.set_mode(refresh_mode);
    Ok(())
}
