use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn get_table_preferences(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64, table_id: String,
) -> Result<Option<String>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .table_preferences(provider_id, &table_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_table_preferences(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64, table_id: String,
    preferences_json: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .save_table_preferences(provider_id, &table_id, &preferences_json)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_default_table_preferences(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64, table_id: String,
) -> Result<String, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    let preferences = core
        .provider_service
        .default_table_preferences(provider_id, &table_id)
        .await?;

    serde_json::to_string(&preferences).map_err(|e| ErrorResponse {
        error: format!("Failed to serialize default preferences: {e}"),
        details: None,
        code: None,
    })
}
