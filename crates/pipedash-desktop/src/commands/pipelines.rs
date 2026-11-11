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
use pipedash_plugin_api::TriggerResult;
use tauri::State;

use crate::error::ErrorResponse;

fn unavailable(error: String) -> ErrorResponse {
    ErrorResponse {
        error,
        details: None,
        code: None,
    }
}

#[tauri::command]
pub async fn get_cached_pipelines(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: Option<i64>,
) -> Result<Vec<Pipeline>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .get_cached_pipelines(provider_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn fetch_run_history(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, query: RunHistoryQuery,
) -> Result<RunHistoryPage, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .query_run_history(&pipeline_id, query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_workflow_run_details(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<PipelineRun, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .fetch_run_details(&pipeline_id, run_number)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn prepare_pipeline_trigger(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, rerun_of: Option<i64>,
    inputs: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<TriggerDraft, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .prepare_trigger(&pipeline_id, rerun_of, inputs)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn trigger_pipeline(
    maybe_core: State<'_, crate::MaybeCoreContext>, params: TriggerParams,
) -> Result<TriggerResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .trigger_pipeline(params)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn cancel_pipeline_run(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_cancellation_targets(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_ids: Vec<String>,
) -> Result<CancellationTargets, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .cancellation_targets(pipeline_ids)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn cancel_pipeline_runs(
    maybe_core: State<'_, crate::MaybeCoreContext>, targets: Vec<RunTarget>,
) -> Result<CancellationBatch, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .cancel_runs(targets)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn force_cancel_pipeline_run(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .force_cancel_run(&pipeline_id, run_number)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn review_pipeline_run(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
    decision: pipedash_plugin_api::ReviewDecision,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .review_run(&pipeline_id, run_number, decision)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn rerun_failed_jobs(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .rerun_failed_jobs(&pipeline_id, run_number)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn rerun_pipeline_run(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<TriggerResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .rerun_run(&pipeline_id, run_number)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn enable_pipeline(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(unavailable)?;
    core.pipeline_service
        .enable_pipeline(&pipeline_id)
        .await
        .map_err(Into::into)
}
