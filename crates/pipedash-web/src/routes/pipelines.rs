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
use pipedash_core::application::TriggerDraft;
use pipedash_core::domain::{
    CancellationBatch,
    CancellationTargets,
    Pipeline,
    PipelineRun,
    RunHistoryPage,
    RunHistoryQuery,
    RunTarget,
    TriggerParams,
};
use pipedash_plugin_api::{
    ReviewDecision,
    TriggerResult,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListPipelinesQuery {
    pub provider_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct TriggerPipelineRequest {
    pub workflow_id: String,
    #[serde(default)]
    pub inputs: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct PrepareTriggerRequest {
    #[serde(default)]
    pub rerun_of: Option<i64>,
    #[serde(default)]
    pub inputs: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewRunRequest {
    pub decision: ReviewDecision,
}

#[derive(Debug, Deserialize)]
pub struct CancellationTargetsRequest {
    pub pipeline_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CancelRunsRequest {
    pub targets: Vec<RunTarget>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/cached", get(list_pipelines))
        .route("/cancellation-targets", post(cancellation_targets))
        .route("/cancel-runs", post(cancel_runs))
        .route("/{id}/runs/query", post(query_run_history))
        .route("/{id}/runs/{run_number}", get(get_run_details))
        .route("/{id}/trigger", post(trigger_pipeline))
        .route("/{id}/trigger/prepare", post(prepare_trigger))
        .route("/{id}/runs/{run_number}/cancel", post(cancel_run))
        .route(
            "/{id}/runs/{run_number}/force-cancel",
            post(force_cancel_run),
        )
        .route("/{id}/runs/{run_number}/review", post(review_run))
        .route(
            "/{id}/runs/{run_number}/rerun-failed-jobs",
            post(rerun_failed_jobs),
        )
        .route("/{id}/runs/{run_number}/rerun", post(rerun_run))
        .route("/{id}/enable", post(enable_pipeline))
}

async fn list_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let pipeline_service = state.pipeline_service().await?;
    let pipelines = pipeline_service
        .get_cached_pipelines(query.provider_id)
        .await?;
    Ok(Json(pipelines))
}

async fn query_run_history(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Json(query): Json<RunHistoryQuery>,
) -> ApiResult<Json<RunHistoryPage>> {
    let pipeline_service = state.pipeline_service().await?;
    let page = pipeline_service
        .query_run_history(&pipeline_id, query)
        .await?;
    Ok(Json(page))
}

async fn get_run_details(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<Json<PipelineRun>> {
    let pipeline_service = state.pipeline_service().await?;
    let run = pipeline_service
        .fetch_run_details(&pipeline_id, run_number)
        .await?;

    Ok(Json(run))
}

async fn prepare_trigger(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Json(req): Json<PrepareTriggerRequest>,
) -> ApiResult<Json<TriggerDraft>> {
    let pipeline_service = state.pipeline_service().await?;
    let draft = pipeline_service
        .prepare_trigger(&pipeline_id, req.rerun_of, req.inputs)
        .await?;
    Ok(Json(draft))
}

async fn trigger_pipeline(
    State(state): State<AppState>, Path(_pipeline_id): Path<String>,
    Json(req): Json<TriggerPipelineRequest>,
) -> ApiResult<Json<TriggerResult>> {
    let pipeline_service = state.pipeline_service().await?;
    let params = TriggerParams {
        workflow_id: req.workflow_id,
        inputs: req.inputs,
    };

    Ok(Json(pipeline_service.trigger_pipeline(params).await?))
}

async fn cancel_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<()> {
    let pipeline_service = state.pipeline_service().await?;
    pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await?;
    Ok(())
}

async fn cancellation_targets(
    State(state): State<AppState>, Json(req): Json<CancellationTargetsRequest>,
) -> ApiResult<Json<CancellationTargets>> {
    let pipeline_service = state.pipeline_service().await?;
    Ok(Json(
        pipeline_service
            .cancellation_targets(req.pipeline_ids)
            .await?,
    ))
}

async fn cancel_runs(
    State(state): State<AppState>, Json(req): Json<CancelRunsRequest>,
) -> ApiResult<Json<CancellationBatch>> {
    let pipeline_service = state.pipeline_service().await?;
    Ok(Json(pipeline_service.cancel_runs(req.targets).await?))
}

async fn force_cancel_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<()> {
    state
        .pipeline_service()
        .await?
        .force_cancel_run(&pipeline_id, run_number)
        .await?;
    Ok(())
}

async fn review_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
    Json(request): Json<ReviewRunRequest>,
) -> ApiResult<()> {
    let pipeline_service = state.pipeline_service().await?;
    pipeline_service
        .review_run(&pipeline_id, run_number, request.decision)
        .await?;
    Ok(())
}

async fn rerun_failed_jobs(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<()> {
    let pipeline_service = state.pipeline_service().await?;
    pipeline_service
        .rerun_failed_jobs(&pipeline_id, run_number)
        .await?;
    Ok(())
}

async fn rerun_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<Json<TriggerResult>> {
    let pipeline_service = state.pipeline_service().await?;
    Ok(Json(
        pipeline_service.rerun_run(&pipeline_id, run_number).await?,
    ))
}

async fn enable_pipeline(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
) -> ApiResult<()> {
    let pipeline_service = state.pipeline_service().await?;
    pipeline_service.enable_pipeline(&pipeline_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
