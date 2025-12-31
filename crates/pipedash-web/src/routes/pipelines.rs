use axum::{
    extract::{
        Path,
        Query,
        State,
    },
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::domain::{
    PaginatedRunHistory,
    Pipeline,
    PipelineRun,
    TriggerParams,
};
use pipedash_plugin_api::WorkflowParameter;
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
pub struct ListPipelinesQuery {
    pub provider_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LazyPipelinesQuery {
    pub provider_id: Option<i64>,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_lazy_page_size")]
    pub page_size: usize,
}

fn default_lazy_page_size() -> usize {
    20
}

#[derive(Debug, Deserialize)]
pub struct RunHistoryQuery {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

#[derive(Debug, Deserialize)]
pub struct TriggerPipelineRequest {
    pub workflow_id: String,
    #[serde(default)]
    pub inputs: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub run_id: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_pipelines))
        .route("/cached", get(get_cached_pipelines))
        .route("/fresh", get(fetch_fresh_pipelines))
        .route("/lazy", get(list_pipelines_lazy))
        .route("/{id}/runs", get(get_run_history))
        .route("/{id}/runs/{run_number}", get(get_run_details))
        .route("/{id}/trigger", post(trigger_pipeline))
        .route("/{id}/runs/{run_number}/cancel", post(cancel_run))
        .route("/{id}/workflow-params", get(get_workflow_parameters))
}

async fn list_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let pipelines = core
        .pipeline_service
        .get_cached_pipelines(query.provider_id)
        .await?;

    let pipeline_service = core.pipeline_service.clone();
    let provider_id = query.provider_id;
    tokio::spawn(async move {
        let _ = pipeline_service.fetch_pipelines(provider_id).await;
    });

    Ok(Json(pipelines))
}

async fn get_run_history(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Query(query): Query<RunHistoryQuery>,
) -> ApiResult<Json<PaginatedRunHistory>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let result = core
        .pipeline_service
        .fetch_run_history_paginated(&pipeline_id, query.page, query.page_size)
        .await?;

    Ok(Json(result))
}

async fn get_run_details(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<Json<PipelineRun>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let runs = core
        .pipeline_service
        .fetch_run_history(&pipeline_id, 100)
        .await?;

    let run = runs
        .into_iter()
        .find(|r| r.run_number == run_number)
        .ok_or_else(|| {
            pipedash_core::domain::DomainError::PipelineNotFound(format!(
                "Run {} not found for pipeline {}",
                run_number, pipeline_id
            ))
        })?;

    Ok(Json(run))
}

async fn trigger_pipeline(
    State(state): State<AppState>, Path(_pipeline_id): Path<String>,
    Json(req): Json<TriggerPipelineRequest>,
) -> ApiResult<Json<TriggerResponse>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let params = TriggerParams {
        workflow_id: req.workflow_id,
        inputs: req.inputs,
    };

    let run_id = core.pipeline_service.trigger_pipeline(params).await?;

    Ok(Json(TriggerResponse { run_id }))
}

async fn cancel_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await?;
    Ok(())
}

async fn get_cached_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let pipelines = core
        .pipeline_service
        .get_cached_pipelines(query.provider_id)
        .await?;
    Ok(Json(pipelines))
}

async fn fetch_fresh_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let pipelines = core
        .pipeline_service
        .fetch_pipelines(query.provider_id)
        .await?;
    Ok(Json(pipelines))
}

async fn list_pipelines_lazy(
    State(state): State<AppState>, Query(query): Query<LazyPipelinesQuery>,
) -> ApiResult<Json<pipedash_plugin_api::PaginatedResponse<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let result = core
        .pipeline_service
        .fetch_pipelines_lazy(query.provider_id, query.page, query.page_size)
        .await?;
    Ok(Json(result))
}

async fn get_workflow_parameters(
    State(state): State<AppState>, Path(workflow_id): Path<String>,
) -> ApiResult<Json<Vec<WorkflowParameter>>> {
    let parts: Vec<&str> = workflow_id.split("__").collect();
    if parts.len() < 2 {
        return Err(AppError::bad_request(format!(
            "Invalid workflow ID format '{}'. Expected format: 'provider__id__...'",
            workflow_id
        )));
    }

    let provider_id: i64 = parts[1].parse().map_err(|_| {
        AppError::bad_request(format!(
            "Invalid provider ID '{}' in workflow ID. Expected numeric ID.",
            parts[1]
        ))
    })?;

    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let params = core
        .provider_service
        .get_workflow_parameters(provider_id, &workflow_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch workflow parameters: {e}")))?;

    Ok(Json(params))
}
