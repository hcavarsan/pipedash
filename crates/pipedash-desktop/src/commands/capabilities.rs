use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn get_provider_table_schema(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64,
) -> Result<pipedash_plugin_api::schema::TableSchema, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    core.provider_service
        .get_provider_table_schema(provider_id)
        .await
        .map_err(Into::into)
}
