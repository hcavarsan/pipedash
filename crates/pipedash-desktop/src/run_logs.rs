use std::sync::Arc;

use pipedash_core::application::LogService;
use pipedash_core::domain::{
    LogLinesPage,
    LogSearchQuery,
    LogSearchResult,
    RunLogDocument,
    RunLogTree,
};
use tauri::State;

use crate::commands::ErrorResponse;
use crate::desktop_lifecycle::OperationLease;
use crate::MaybeCoreContext;

pub(crate) async fn log_service(
    maybe_core: &MaybeCoreContext,
) -> Result<Arc<LogService>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(Arc::clone(&core.log_service))
}

#[tauri::command]
pub async fn get_run_log_tree(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String,
    run_number: i64,
) -> Result<RunLogTree, ErrorResponse> {
    let service = log_service(&maybe_core).await?;
    Ok(service.run_log_tree(&pipeline_id, run_number).await?)
}

#[tauri::command]
pub async fn open_run_log(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String,
    run_number: i64, node_id: String, retry: bool,
) -> Result<RunLogDocument, ErrorResponse> {
    let service = log_service(&maybe_core).await?;
    Ok(service
        .open_document(&pipeline_id, run_number, &node_id, retry)
        .await?)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn read_run_log_lines(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String,
    run_number: i64, node_id: String, revision: i64, start: u32, count: u32, max_line_units: u32,
) -> Result<LogLinesPage, ErrorResponse> {
    let service = log_service(&maybe_core).await?;
    Ok(service
        .read_lines(
            &pipeline_id,
            run_number,
            &node_id,
            revision,
            start,
            count,
            max_line_units,
        )
        .await?)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn search_run_log(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String,
    run_number: i64, node_id: String, revision: i64, session_id: String, query: LogSearchQuery,
) -> Result<LogSearchResult, ErrorResponse> {
    let service = log_service(&maybe_core).await?;
    Ok(service
        .search(
            &pipeline_id,
            run_number,
            &node_id,
            revision,
            &session_id,
            &query,
        )
        .await?)
}

#[tauri::command]
pub async fn clear_run_logs_cache(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    Ok(log_service(&maybe_core).await?.clear().await?)
}
