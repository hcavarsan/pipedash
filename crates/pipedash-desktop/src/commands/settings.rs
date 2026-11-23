use pipedash_core::infrastructure::{
    GeneralConfig,
    GeneralSettingsUpdate,
};
use tauri::State;

use crate::error::ErrorResponse;

fn core_error(error: String) -> ErrorResponse {
    ErrorResponse {
        error,
        details: None,
        code: None,
    }
}

#[tauri::command]
pub async fn get_general_settings(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<GeneralConfig, ErrorResponse> {
    let core = maybe_core.get().await.map_err(core_error)?;
    Ok(core
        .provider_service
        .general_settings(&core.config_state)
        .await)
}

#[tauri::command]
pub async fn update_general_settings(
    app: tauri::AppHandle, maybe_core: State<'_, crate::MaybeCoreContext>,
    update: GeneralSettingsUpdate,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(core_error)?;
    core.provider_service
        .update_general_settings(&core.config_state, update)
        .await?;
    crate::local_notifications::settings::schedule_sync(&app);
    Ok(())
}
