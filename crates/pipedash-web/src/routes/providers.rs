mod capabilities;
mod discovery;
mod draft;

use axum::{
    extract::{
        Path,
        State,
    },
    routing::{
        delete,
        get,
        post,
        put,
    },
    Json,
    Router,
};
use pipedash_core::application::services::provider_service::dto::ProviderView;
use pipedash_core::domain::{
    ProviderImportResult,
    RefreshResult,
    SignInStart,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateRefreshIntervalRequest {
    pub refresh_interval: i64,
}

#[derive(Debug, Deserialize)]
pub struct ImportProvidersRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ReconnectProviderRequest {
    pub flow_id: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_providers))
        .route("/export", get(export_providers))
        .route("/import", post(import_providers))
        .route("/{id}", get(get_provider))
        .route("/{id}", delete(remove_provider))
        .route("/{id}/refresh", post(refresh_provider))
        .route("/{id}/sign-in", post(start_provider_sign_in))
        .route("/{id}/sign-in/complete", post(reconnect_provider))
        .route("/{id}/refresh-interval", put(update_refresh_interval))
        .route(
            "/{id}/table-schema",
            get(capabilities::get_provider_table_schema),
        )
        .route("/organizations", post(discovery::fetch_organizations))
        .route("/permissions/check", post(capabilities::check_permissions))
        .route("/preview", post(discovery::preview_pipelines))
        .route("/field-options", post(discovery::get_field_options))
        .route("/preview/ids", post(draft::matching_pipeline_ids))
        .route("/draft", post(draft::save_draft))
        .route("/draft/validate", post(draft::validate_draft))
}

async fn start_provider_sign_in(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<SignInStart>> {
    let start = state
        .provider_service()
        .await?
        .start_provider_sign_in(id)
        .await?;
    Ok(Json(SignInStart {
        manual: true,
        ..start
    }))
}

async fn reconnect_provider(
    State(state): State<AppState>, Path(id): Path<i64>, Json(req): Json<ReconnectProviderRequest>,
) -> ApiResult<()> {
    state
        .provider_service()
        .await?
        .reconnect_provider(id, &req.flow_id)
        .await?;
    Ok(())
}

async fn refresh_provider(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<RefreshResult>> {
    let pipeline_service = state.pipeline_service().await?;
    Ok(Json(pipeline_service.refresh_provider(id).await))
}

async fn export_providers(State(state): State<AppState>) -> ApiResult<Json<String>> {
    let provider_service = state.provider_service().await?;
    let toml = provider_service.export_providers_toml().await?;

    Ok(Json(toml))
}

async fn import_providers(
    State(state): State<AppState>, Json(req): Json<ImportProvidersRequest>,
) -> ApiResult<Json<ProviderImportResult>> {
    let provider_service = state.provider_service().await?;
    let result = provider_service.import_providers_toml(&req.content).await?;

    Ok(Json(result))
}

async fn list_providers(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<pipedash_core::ProviderSummary>>> {
    let provider_service = state.provider_service().await?;
    let providers = provider_service.list_providers().await?;

    Ok(Json(providers))
}

async fn get_provider(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<ProviderView>> {
    let provider_service = state.provider_service().await?;
    let provider = provider_service.provider_for_editing(id).await?;
    Ok(Json(ProviderView::new(id, provider)))
}

async fn remove_provider(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<()> {
    let provider_service = state.provider_service().await?;
    provider_service.remove_provider(id).await?;
    Ok(())
}

async fn update_refresh_interval(
    State(state): State<AppState>, Path(id): Path<i64>,
    Json(req): Json<UpdateRefreshIntervalRequest>,
) -> ApiResult<()> {
    let provider_service = state.provider_service().await?;
    provider_service
        .update_provider_refresh_interval(id, req.refresh_interval)
        .await?;
    Ok(())
}
