use pipedash_core::infrastructure::GeneralSettingsUpdate;
use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn update_general_settings(
    app: tauri::AppHandle, maybe_core: State<'_, crate::MaybeCoreContext>,
    update: GeneralSettingsUpdate,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .update_general_settings(&core.config_state, update)
        .await?;
    crate::local_notifications::settings::schedule_sync(&app);
    Ok(())
}
