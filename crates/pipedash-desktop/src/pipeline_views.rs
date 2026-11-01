use std::sync::Arc;

use pipedash_core::application::PipelineViewService;
use pipedash_core::domain::{
    CreateViewInput,
    LegacyImportResult,
    LegacyMarks,
    PinKind,
    PinnedItem,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    ViewMember,
    ViewPatch,
    ViewRef,
};
use tauri::State;

use crate::commands::ErrorResponse;
use crate::desktop_lifecycle::OperationLease;
use crate::MaybeCoreContext;

async fn view_service(
    maybe_core: &MaybeCoreContext,
) -> Result<Arc<PipelineViewService>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(Arc::clone(&core.pipeline_view_service))
}

#[tauri::command]
pub async fn list_pipeline_views(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>,
) -> Result<Vec<PipelineView>, ErrorResponse> {
    Ok(view_service(&maybe_core).await?.list_views().await?)
}

#[tauri::command]
pub async fn get_pipeline_view_page(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, query: PipelineViewQuery,
) -> Result<PipelineViewPage, ErrorResponse> {
    Ok(view_service(&maybe_core).await?.page(query).await?)
}

#[tauri::command]
pub async fn create_pipeline_view(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, input: CreateViewInput,
) -> Result<PipelineView, ErrorResponse> {
    Ok(view_service(&maybe_core).await?.create_view(input).await?)
}

#[tauri::command]
pub async fn update_pipeline_view(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, id: ViewRef,
    patch: ViewPatch,
) -> Result<PipelineView, ErrorResponse> {
    Ok(view_service(&maybe_core)
        .await?
        .update_view(id, patch)
        .await?)
}

#[tauri::command]
pub async fn delete_pipeline_view(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, id: ViewRef,
) -> Result<(), ErrorResponse> {
    Ok(view_service(&maybe_core).await?.delete_view(id).await?)
}

#[tauri::command]
pub async fn reorder_pipeline_views(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, order: Vec<ViewRef>,
) -> Result<(), ErrorResponse> {
    Ok(view_service(&maybe_core)
        .await?
        .reorder_views(order)
        .await?)
}

#[tauri::command]
pub async fn list_pipeline_view_members(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, id: ViewRef,
) -> Result<Vec<ViewMember>, ErrorResponse> {
    Ok(view_service(&maybe_core).await?.list_members(id).await?)
}

#[tauri::command]
pub async fn set_pipeline_view_membership(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, id: ViewRef,
    pipeline_id: String, member: bool,
) -> Result<(), ErrorResponse> {
    Ok(view_service(&maybe_core)
        .await?
        .set_membership(id, &pipeline_id, member)
        .await?)
}

#[tauri::command]
pub async fn set_pipeline_hidden(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, pipeline_id: String,
    hidden: bool,
) -> Result<(), ErrorResponse> {
    Ok(view_service(&maybe_core)
        .await?
        .set_hidden(&pipeline_id, hidden)
        .await?)
}

#[tauri::command]
pub async fn import_legacy_pipeline_marks(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, marks: LegacyMarks,
) -> Result<LegacyImportResult, ErrorResponse> {
    Ok(view_service(&maybe_core)
        .await?
        .import_legacy_marks(marks)
        .await?)
}

#[tauri::command]
pub async fn list_pins(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>,
) -> Result<Vec<PinnedItem>, ErrorResponse> {
    Ok(view_service(&maybe_core).await?.list_pins().await?)
}

#[tauri::command]
pub async fn set_pin(
    _operation: OperationLease, maybe_core: State<'_, MaybeCoreContext>, kind: PinKind,
    r#ref: String, pinned: bool,
) -> Result<(), ErrorResponse> {
    Ok(view_service(&maybe_core)
        .await?
        .set_pin(kind, &r#ref, pinned)
        .await?)
}
