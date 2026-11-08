use pipedash_core::application::services::cache_service::CacheStats;
use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn get_cache_stats(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<CacheStats, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core.cache_stats().await?)
}

#[tauri::command]
pub async fn clear_pipelines_cache(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<usize, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.pipeline_service
        .clear_pipelines_cache()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_all_run_history_caches(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.pipeline_service.clear_all_run_history_caches().await?;
    Ok(())
}

#[tauri::command]
pub async fn clear_workflow_params_cache(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.pipeline_service
        .clear_workflow_params_cache()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_all_caches(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core.clear_all_caches().await?)
}
