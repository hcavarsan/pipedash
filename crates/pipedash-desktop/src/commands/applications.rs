use std::sync::Arc;

use pipedash_core::application::CorrelationService;
use pipedash_core::domain::application::dto::{
    ApplicationDetail,
    ApplicationList,
    ApplicationListQuery,
    ApplicationRef,
    ChangesPage,
    CommitLocations,
    DeployChanges,
    Suggestion,
};
use pipedash_core::domain::application::metrics::{
    ApplicationMetrics,
    ApplicationMetricsQuery,
};
use pipedash_core::domain::application::overrides::{
    ApplicationEdit,
    CreateApplicationInput,
};
use tauri::State;

use crate::error::ErrorResponse;
use crate::MaybeCoreContext;

async fn correlation_service(
    maybe_core: &MaybeCoreContext,
) -> Result<Arc<CorrelationService>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(Arc::clone(&core.correlation_service))
}

#[tauri::command]
pub async fn list_applications(
    maybe_core: State<'_, MaybeCoreContext>, query: ApplicationListQuery,
) -> Result<ApplicationList, ErrorResponse> {
    Ok(correlation_service(&maybe_core).await?.list(query).await?)
}

#[tauri::command]
pub async fn get_application(
    maybe_core: State<'_, MaybeCoreContext>, id: i64,
) -> Result<ApplicationDetail, ErrorResponse> {
    Ok(correlation_service(&maybe_core).await?.detail(id).await?)
}

#[tauri::command]
pub async fn get_application_changes(
    maybe_core: State<'_, MaybeCoreContext>, id: i64, cursor: Option<String>, limit: usize,
) -> Result<ChangesPage, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .changes(id, cursor, limit)
        .await?)
}

#[tauri::command]
pub async fn get_application_metrics(
    maybe_core: State<'_, MaybeCoreContext>, query: ApplicationMetricsQuery,
) -> Result<ApplicationMetrics, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .metrics(query)
        .await?)
}

#[tauri::command]
pub async fn create_application(
    maybe_core: State<'_, MaybeCoreContext>, input: CreateApplicationInput,
) -> Result<ApplicationDetail, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .create_application(input, None)
        .await?)
}

#[tauri::command]
pub async fn edit_application(
    maybe_core: State<'_, MaybeCoreContext>, id: i64, edit: ApplicationEdit,
) -> Result<ApplicationDetail, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .apply_edit(id, edit, None)
        .await?)
}

#[tauri::command]
pub async fn delete_application(
    maybe_core: State<'_, MaybeCoreContext>, id: i64,
) -> Result<(), ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .delete_application(id)
        .await?)
}

#[tauri::command]
pub async fn revert_application_override(
    maybe_core: State<'_, MaybeCoreContext>, id: i64, override_id: i64,
) -> Result<ApplicationDetail, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .revert_override(id, override_id)
        .await?)
}

#[tauri::command]
pub async fn list_application_suggestions(
    maybe_core: State<'_, MaybeCoreContext>,
) -> Result<Vec<Suggestion>, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .suggestions()
        .await?)
}

#[tauri::command]
pub async fn accept_application_suggestions(
    maybe_core: State<'_, MaybeCoreContext>, ids: Vec<String>,
) -> Result<(), ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .accept_suggestions(ids, None)
        .await?)
}

#[tauri::command]
pub async fn get_pipeline_applications(
    maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String,
) -> Result<Vec<ApplicationRef>, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .pipeline_applications(&pipeline_id)
        .await?)
}

#[tauri::command]
pub async fn get_commit_locations(
    maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<CommitLocations, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .commit_locations(&pipeline_id, run_number)
        .await?)
}

#[tauri::command]
pub async fn get_deploy_changes(
    maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<DeployChanges, ErrorResponse> {
    Ok(correlation_service(&maybe_core)
        .await?
        .deploy_changes(&pipeline_id, run_number)
        .await?)
}
