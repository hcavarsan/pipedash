use axum::{
    extract::{
        Path,
        State,
    },
    routing::{
        get,
        post,
        put,
    },
    Json,
    Router,
};
use pipedash_core::domain::{
    GlobalMetricsConfig,
    MetricsConfig,
    MetricsDashboard,
    MetricsDashboardQuery,
    MetricsStats,
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
pub struct UpdateGlobalMetricsConfigRequest {
    pub enabled: bool,
    pub default_retention_days: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePipelineMetricsConfigRequest {
    pub enabled: bool,
    pub retention_days: i64,
}

#[derive(Debug, Deserialize)]
pub struct FlushMetricsRequest {
    pub pipeline_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FlushMetricsResponse {
    pub flushed: usize,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_global_metrics_config))
        .route("/config", put(update_global_metrics_config))
        .route("/pipelines/{id}/config", get(get_pipeline_metrics_config))
        .route(
            "/pipelines/{id}/config",
            put(update_pipeline_metrics_config),
        )
        .route("/dashboard", post(get_metrics_dashboard))
        .route("/storage/stats", get(get_metrics_storage_stats))
        .route("/flush", post(flush_pipeline_metrics))
}

async fn get_global_metrics_config(
    State(state): State<AppState>,
) -> ApiResult<Json<GlobalMetricsConfig>> {
    let core = state.core().await?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let config = metrics_service.get_global_config().await?;
    Ok(Json(config))
}

async fn update_global_metrics_config(
    State(state): State<AppState>, Json(req): Json<UpdateGlobalMetricsConfigRequest>,
) -> ApiResult<()> {
    let core = state.core().await?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    metrics_service
        .update_global_config(req.enabled, req.default_retention_days)
        .await?;
    Ok(())
}

async fn get_pipeline_metrics_config(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
) -> ApiResult<Json<MetricsConfig>> {
    let core = state.core().await?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let config = metrics_service
        .get_effective_pipeline_config(&pipeline_id)
        .await?;
    Ok(Json(config))
}

async fn update_pipeline_metrics_config(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Json(req): Json<UpdatePipelineMetricsConfigRequest>,
) -> ApiResult<()> {
    let core = state.core().await?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    metrics_service
        .update_pipeline_config(&pipeline_id, req.enabled, req.retention_days)
        .await?;
    Ok(())
}

async fn get_metrics_dashboard(
    State(state): State<AppState>, Json(query): Json<MetricsDashboardQuery>,
) -> ApiResult<Json<MetricsDashboard>> {
    let core = state.core().await?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;

    Ok(Json(metrics_service.dashboard(query).await?))
}

async fn get_metrics_storage_stats(State(state): State<AppState>) -> ApiResult<Json<MetricsStats>> {
    let core = state.core().await?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let stats = metrics_service.get_storage_stats().await?;
    Ok(Json(stats))
}

async fn flush_pipeline_metrics(
    State(state): State<AppState>, Json(req): Json<FlushMetricsRequest>,
) -> ApiResult<Json<FlushMetricsResponse>> {
    let core = state.core().await?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let flushed = metrics_service
        .flush_metrics(req.pipeline_id.as_deref(), false)
        .await?;
    Ok(Json(FlushMetricsResponse { flushed }))
}

#[cfg(test)]
mod tests;
