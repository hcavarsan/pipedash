use std::path::Path;

use pipedash_core::application::services::storage_service;
use pipedash_core::application::services::storage_service::config::{
    self as storage_config,
    ConfigContentResponse,
    StorageConfigResponse,
    StoragePathsResponse,
};
use pipedash_core::infrastructure::{
    ConfigAnalysis,
    PipedashConfig,
    StorageManager,
};
use pipedash_core::CoreContext;
use tauri::State;

use crate::error::ErrorResponse;
use crate::AppDataDir;

pub(super) async fn open_current(
    app_data_dir: &Path, core: Option<&CoreContext>,
) -> Result<StorageManager, ErrorResponse> {
    let config = storage_config::load_config(app_data_dir).await?;
    let token_store = core.map(|core| core.token_store.clone());

    Ok(storage_service::open_manager(config, token_store, true).await?)
}

#[tauri::command]
pub async fn get_storage_config(
    app_data_dir: State<'_, AppDataDir>,
) -> Result<StorageConfigResponse, ErrorResponse> {
    let config = storage_config::load_config(&app_data_dir.0).await?;
    Ok(StorageConfigResponse::new(config))
}

#[tauri::command]
pub async fn get_config_content(
    app_data_dir: State<'_, AppDataDir>,
) -> Result<ConfigContentResponse, ErrorResponse> {
    Ok(storage_config::read_config_content(&app_data_dir.0.join("config.toml")).await?)
}

#[tauri::command]
pub async fn save_config_content(
    maybe_core: State<'_, crate::MaybeCoreContext>, app_data_dir: State<'_, AppDataDir>,
    content: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.0.read().await.clone();

    storage_config::save_config_content(
        &app_data_dir.0.join("config.toml"),
        &content,
        &crate::plugin_registry::plugin_registry(),
        core.as_deref(),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn analyze_config(
    maybe_core: State<'_, crate::MaybeCoreContext>, app_data_dir: State<'_, AppDataDir>,
    content: String,
) -> Result<ConfigAnalysis, ErrorResponse> {
    let core = maybe_core.0.read().await.clone();
    let manager = open_current(&app_data_dir.0, core.as_deref()).await?;
    let event_bus = core.as_ref().map(|core| core.event_bus.clone());

    Ok(storage_config::analyze_config(
        &content,
        &manager,
        event_bus,
        &crate::plugin_registry::plugin_registry(),
    )
    .await?)
}

#[tauri::command]
pub async fn get_storage_paths(
    app_data_dir: State<'_, AppDataDir>,
) -> Result<StoragePathsResponse, ErrorResponse> {
    let config_path = app_data_dir.0.join("config.toml");
    let data_dir = storage_config::effective_data_dir(&config_path, &app_data_dir.0).await?;

    Ok(StoragePathsResponse::new(&config_path, &data_dir))
}

#[tauri::command]
pub async fn get_default_data_dir() -> Result<String, ErrorResponse> {
    Ok(PipedashConfig::default_data_dir().display().to_string())
}
