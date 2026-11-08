use std::collections::HashMap;

use pipedash_core::application::services::provider_service::dto::PermissionCheckResult;
use pipedash_core::application::ProviderService;
use pipedash_core::domain::PaginatedAvailablePipelines;
use tauri::State;

use crate::error::ErrorResponse;

async fn available_plugins(
    maybe_core: &crate::MaybeCoreContext,
) -> Vec<pipedash_plugin_api::PluginMetadata> {
    let core = maybe_core.0.read().await.clone();
    if let Some(core) = core {
        return core.provider_service.list_available_plugins();
    }

    ProviderService::available_plugins(&crate::plugin_registry::plugin_registry())
}

#[tauri::command]
pub async fn get_available_plugins(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<Vec<pipedash_plugin_api::PluginMetadata>, ErrorResponse> {
    Ok(available_plugins(&maybe_core).await)
}

#[tauri::command]
pub async fn get_provider_field_options(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String, field_key: String,
    config: HashMap<String, String>,
) -> Result<Vec<String>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .field_options(&provider_type, &field_key, config)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch field options: {e}"),
            details: None,
            code: None,
        })
}

#[tauri::command]
pub async fn fetch_provider_organizations(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>,
) -> Result<Vec<pipedash_plugin_api::Organization>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core
        .provider_service
        .discover_organizations(&provider_type, config)
        .await?)
}

#[tauri::command]
pub async fn preview_provider_pipelines(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>, org: Option<String>, search: Option<String>,
    page: Option<usize>, page_size: Option<usize>,
) -> Result<PaginatedAvailablePipelines, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core
        .provider_service
        .preview_pipelines(&provider_type, config, org, search, page, page_size)
        .await?)
}

#[tauri::command]
pub async fn check_provider_permissions(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>,
) -> Result<PermissionCheckResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    Ok(core
        .provider_service
        .check_permissions(&provider_type, config)
        .await?)
}
