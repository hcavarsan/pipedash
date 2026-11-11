use pipedash_core::domain::{
    ProviderConfig,
    ProviderImportResult,
    ProviderSummary,
};
use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn list_providers(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<Vec<ProviderSummary>, ErrorResponse> {
    tracing::debug!("[list_providers] Command invoked");
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    let result = core.provider_service.list_providers().await;
    match &result {
        Ok(providers) => {
            tracing::debug!("[list_providers] Returning {} providers", providers.len())
        }
        Err(e) => tracing::error!("[list_providers] Error: {:?}", e),
    }
    result.map_err(Into::into)
}

#[tauri::command]
pub async fn get_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64,
) -> Result<ProviderConfig, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .provider_for_editing(id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_provider_refresh_interval(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64, refresh_interval: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .update_provider_refresh_interval(id, refresh_interval)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn remove_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .remove_provider(id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn export_providers(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<String, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .export_providers_toml()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn import_providers(
    maybe_core: State<'_, crate::MaybeCoreContext>, content: String,
) -> Result<ProviderImportResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .import_providers_toml(&content)
        .await
        .map_err(Into::into)
}
