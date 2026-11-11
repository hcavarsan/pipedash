mod migration;
pub(super) mod session;
mod setup;

use std::path::PathBuf;

use axum::{
    extract::State,
    routing::{
        get,
        post,
        put,
    },
    Json,
    Router,
};
use pipedash_core::application::services::storage_service::config::{
    self as storage_config,
    ConfigContentResponse,
    StorageConfigResponse,
    StoragePathsResponse,
};
use pipedash_core::application::services::storage_service::vault::{
    vault_password_status,
    VaultPasswordStatus,
};
use pipedash_core::infrastructure::{
    ConfigAnalysis,
    ConfigLoader,
    PipedashConfig,
};
use serde::Deserialize;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SaveConfigContentRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeConfigRequest {
    pub new_content: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/draft", post(setup::prepare_draft))
        .route("/apply", post(setup::apply_draft))
        .route("/config", get(get_storage_config))
        .route("/config/content", get(get_config_content))
        .route("/config/content", put(save_config_content))
        .route("/config/analyze", post(analyze_config))
        .route("/paths", get(get_storage_paths))
        .route("/default-data-dir", get(get_default_data_dir))
        .route("/test-connection", post(migration::test_storage_connection))
        .route("/vault-password-status", get(get_vault_password_status))
        .route("/migration/execute", post(migration::execute_migration))
}

async fn get_vault_password_status() -> Json<VaultPasswordStatus> {
    Json(vault_password_status())
}

async fn config_path() -> ApiResult<PathBuf> {
    tokio::task::spawn_blocking(ConfigLoader::discover_config_path)
        .await
        .map_err(|e| AppError::internal(format!("Background task failed: {e}")))
}

async fn get_storage_config(
    State(state): State<AppState>,
) -> ApiResult<Json<StorageConfigResponse>> {
    let storage_manager = state.storage_manager().await.ok_or_else(|| {
        AppError::not_found("Storage configuration not found - initial setup required")
    })?;

    let mut config = storage_manager.config().clone();
    if let Ok(core) = state.core().await {
        config.general = core.config_state.get().await.general.clone();
    }

    Ok(Json(StorageConfigResponse::new(config)))
}

async fn get_config_content() -> ApiResult<Json<ConfigContentResponse>> {
    let config_path = config_path().await?;
    tracing::debug!("Reading config file from: {:?}", config_path);

    Ok(Json(
        storage_config::read_config_content(&config_path).await?,
    ))
}

async fn save_config_content(
    State(state): State<AppState>, Json(req): Json<SaveConfigContentRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let config_path = config_path().await?;
    let core = state.core().await.ok();

    storage_config::save_config_content(
        &config_path,
        &req.content,
        &state.plugin_registry,
        core.as_ref(),
    )
    .await?;

    Ok(Json(serde_json::json!({"success": true})))
}

async fn analyze_config(
    State(state): State<AppState>, Json(req): Json<AnalyzeConfigRequest>,
) -> ApiResult<Json<ConfigAnalysis>> {
    let storage_manager = state
        .storage_manager()
        .await
        .ok_or_else(|| AppError::not_found("Storage configuration not found"))?;
    let event_bus = state.core().await.ok().map(|core| core.event_bus.clone());

    let analysis = storage_config::analyze_config(
        &req.new_content,
        &storage_manager,
        event_bus,
        &state.plugin_registry,
    )
    .await?;

    Ok(Json(analysis))
}

async fn get_storage_paths(State(state): State<AppState>) -> ApiResult<Json<StoragePathsResponse>> {
    let storage_manager = state
        .storage_manager()
        .await
        .ok_or_else(|| AppError::internal("Storage manager not available"))?;
    let config_path = config_path().await?;

    Ok(Json(StoragePathsResponse::new(
        &config_path,
        &storage_manager.config().data_dir(),
    )))
}

async fn get_default_data_dir() -> Json<String> {
    Json(PipedashConfig::default_data_dir().display().to_string())
}

#[cfg(test)]
mod tests;
