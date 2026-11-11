use axum::{
    extract::State,
    routing::{
        delete,
        get,
    },
    Json,
    Router,
};
use pipedash_core::application::services::cache_service::CacheStats;
use serde::Serialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct CacheClearResponse {
    pub cleared: usize,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stats", get(get_cache_stats))
        .route("/run-history", delete(clear_all_run_history_caches))
        .route("/pipelines", delete(clear_pipelines_cache))
        .route("/workflow-params", delete(clear_workflow_params_cache))
        .route("/run-logs", delete(clear_run_logs_cache))
        .route("/", delete(clear_all_caches))
}

async fn get_cache_stats(State(state): State<AppState>) -> ApiResult<Json<CacheStats>> {
    Ok(Json(state.core().await?.cache_stats().await?))
}

async fn clear_all_run_history_caches(State(state): State<AppState>) -> ApiResult<()> {
    let core = state.core().await?;
    core.pipeline_service.clear_all_run_history_caches().await?;
    Ok(())
}

async fn clear_pipelines_cache(
    State(state): State<AppState>,
) -> ApiResult<Json<CacheClearResponse>> {
    let core = state.core().await?;
    let cleared = core.pipeline_service.clear_pipelines_cache().await?;
    Ok(Json(CacheClearResponse { cleared }))
}

async fn clear_workflow_params_cache(State(state): State<AppState>) -> ApiResult<()> {
    let core = state.core().await?;
    core.pipeline_service.clear_workflow_params_cache().await?;
    Ok(())
}

async fn clear_run_logs_cache(State(state): State<AppState>) -> ApiResult<()> {
    state.log_service().await?.clear().await?;
    Ok(())
}

async fn clear_all_caches(State(state): State<AppState>) -> ApiResult<()> {
    state.core().await?.clear_all_caches().await?;
    Ok(())
}
