use axum::{
    extract::{
        Path,
        State,
    },
    routing::{
        delete,
        get,
    },
    Json,
    Router,
};
use serde::Serialize;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct CacheStats {
    pub pipelines_count: i64,
    pub run_history_count: i64,
    pub workflow_params_count: i64,
    pub metrics_count: i64,
}

#[derive(Debug, Serialize)]
pub struct CacheClearResponse {
    pub cleared: usize,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stats", get(get_cache_stats))
        .route(
            "/run-history/{pipeline_id}",
            delete(clear_run_history_cache),
        )
        .route("/run-history", delete(clear_all_run_history_caches))
        .route("/pipelines", delete(clear_pipelines_cache))
        .route("/workflow-params", delete(clear_workflow_params_cache))
        .route("/", delete(clear_all_caches))
}

async fn get_cache_stats(State(state): State<AppState>) -> ApiResult<Json<CacheStats>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let pipelines_count = core
        .pipeline_service
        .get_pipelines_cache_count()
        .await
        .unwrap_or(0);

    let run_history_count = core
        .pipeline_service
        .get_run_history_cache_count()
        .await
        .unwrap_or(0);

    let workflow_params_count = core
        .pipeline_service
        .get_workflow_params_cache_count()
        .await
        .unwrap_or(0);

    let metrics_count = if let Some(metrics_service) = &core.metrics_service {
        metrics_service
            .get_storage_stats()
            .await
            .map(|stats| stats.total_metrics_count)
            .unwrap_or(0)
    } else {
        0
    };

    Ok(Json(CacheStats {
        pipelines_count,
        run_history_count,
        workflow_params_count,
        metrics_count,
    }))
}

async fn clear_run_history_cache(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service
        .clear_run_history_cache(&pipeline_id)
        .await;
    Ok(())
}

async fn clear_all_run_history_caches(State(state): State<AppState>) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service.clear_all_run_history_caches().await;
    Ok(())
}

async fn clear_pipelines_cache(
    State(state): State<AppState>,
) -> ApiResult<Json<CacheClearResponse>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let cleared = core.pipeline_service.clear_pipelines_cache().await?;
    Ok(Json(CacheClearResponse { cleared }))
}

async fn clear_workflow_params_cache(State(state): State<AppState>) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service.clear_workflow_params_cache().await?;
    Ok(())
}

async fn clear_all_caches(State(state): State<AppState>) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service.clear_all_run_history_caches().await;
    core.pipeline_service.clear_pipelines_cache().await?;
    core.pipeline_service.clear_workflow_params_cache().await?;
    Ok(())
}
