use axum::extract::{
    Path,
    Query,
    State,
};
use axum::http::StatusCode;
use axum::routing::{
    get,
    post,
    put,
};
use axum::{
    Json,
    Router,
};
use pipedash_core::domain::{
    CreateViewInput,
    LegacyImportResult,
    LegacyMarks,
    PinKind,
    PinnedItem,
    PipelineMenuSnapshot,
    PipelineNavigation,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    ViewEditorOptions,
    ViewMember,
    ViewMembershipChange,
    ViewPatch,
    ViewRef,
    ViewSelectionEntry,
    ViewSelectionQuery,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct OrderRequest {
    order: Vec<ViewRef>,
}

#[derive(Deserialize)]
pub struct HiddenRequest {
    pipeline_id: String,
    hidden: bool,
}

#[derive(Deserialize)]
pub struct PinRequest {
    kind: PinKind,
    #[serde(rename = "ref")]
    reference: String,
    pinned: bool,
}

#[derive(Deserialize)]
pub struct MembershipRequest {
    pipeline_id: String,
    member: bool,
}

#[derive(Deserialize)]
pub struct EditorOptionsQuery {
    view: Option<ViewRef>,
}

#[derive(Deserialize)]
pub struct NavigationQuery {
    search: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_views).post(create_view))
        .route("/page", get(page))
        .route("/order", put(reorder_views))
        .route("/hidden", put(set_hidden))
        .route("/pins", get(list_pins).put(set_pin))
        .route("/menu", get(menu))
        .route("/selection", post(selection))
        .route("/editor-options", get(editor_options))
        .route("/navigation", get(navigation))
        .route("/legacy-import", post(import_legacy_marks))
        .route("/{id}", put(update_view).delete(delete_view))
        .route("/{id}/members", get(list_members))
        .route("/{id}/membership", put(set_membership))
        .route("/{id}/memberships", put(set_memberships))
}

async fn list_views(State(state): State<AppState>) -> ApiResult<Json<Vec<PipelineView>>> {
    Ok(Json(
        state.pipeline_view_service().await?.list_views().await?,
    ))
}

async fn create_view(
    State(state): State<AppState>, Json(input): Json<CreateViewInput>,
) -> ApiResult<(StatusCode, Json<PipelineView>)> {
    let view = state
        .pipeline_view_service()
        .await?
        .create_view(input)
        .await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn page(
    State(state): State<AppState>, Query(query): Query<PipelineViewQuery>,
) -> ApiResult<Json<PipelineViewPage>> {
    Ok(Json(
        state.pipeline_view_service().await?.page(query).await?,
    ))
}

async fn menu(State(state): State<AppState>) -> ApiResult<Json<PipelineMenuSnapshot>> {
    Ok(Json(
        state.pipeline_view_service().await?.menu_snapshot().await?,
    ))
}

async fn list_pins(State(state): State<AppState>) -> ApiResult<Json<Vec<PinnedItem>>> {
    Ok(Json(
        state.pipeline_view_service().await?.list_pins().await?,
    ))
}

async fn set_pin(State(state): State<AppState>, Json(req): Json<PinRequest>) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .set_pin(req.kind, &req.reference, req.pinned)
        .await?)
}

async fn reorder_views(
    State(state): State<AppState>, Json(req): Json<OrderRequest>,
) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .reorder_views(req.order)
        .await?)
}

async fn set_hidden(
    State(state): State<AppState>, Json(req): Json<HiddenRequest>,
) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .set_hidden(&req.pipeline_id, req.hidden)
        .await?)
}

async fn import_legacy_marks(
    State(state): State<AppState>, Json(marks): Json<LegacyMarks>,
) -> ApiResult<Json<LegacyImportResult>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .import_legacy_marks(marks)
            .await?,
    ))
}

async fn update_view(
    State(state): State<AppState>, Path(id): Path<ViewRef>, Json(patch): Json<ViewPatch>,
) -> ApiResult<Json<PipelineView>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .update_view(id, patch)
            .await?,
    ))
}

async fn delete_view(State(state): State<AppState>, Path(id): Path<ViewRef>) -> ApiResult<()> {
    Ok(state.pipeline_view_service().await?.delete_view(id).await?)
}

async fn list_members(
    State(state): State<AppState>, Path(id): Path<ViewRef>,
) -> ApiResult<Json<Vec<ViewMember>>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .list_members(id)
            .await?,
    ))
}

async fn set_membership(
    State(state): State<AppState>, Path(id): Path<ViewRef>, Json(req): Json<MembershipRequest>,
) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .set_membership(id, &req.pipeline_id, req.member)
        .await?)
}

async fn selection(
    State(state): State<AppState>, Json(query): Json<ViewSelectionQuery>,
) -> ApiResult<Json<Vec<ViewSelectionEntry>>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .selection(query)
            .await?,
    ))
}

async fn set_memberships(
    State(state): State<AppState>, Path(id): Path<ViewRef>,
    Json(change): Json<ViewMembershipChange>,
) -> ApiResult<Json<Vec<ViewSelectionEntry>>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .set_memberships(id, change)
            .await?,
    ))
}

async fn editor_options(
    State(state): State<AppState>, Query(query): Query<EditorOptionsQuery>,
) -> ApiResult<Json<ViewEditorOptions>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .editor_options(query.view)
            .await?,
    ))
}

async fn navigation(
    State(state): State<AppState>, Query(query): Query<NavigationQuery>,
) -> ApiResult<Json<PipelineNavigation>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .navigation(query.search)
            .await?,
    ))
}

#[cfg(test)]
mod tests;
