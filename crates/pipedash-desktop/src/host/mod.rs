use std::sync::Arc;

use pipedash_core::CoreContext;
use serde::Serialize;
use tauri::AppHandle;

use crate::error::ErrorResponse;

#[cfg(desktop)]
pub(crate) mod desktop;
#[cfg(not(desktop))]
mod unsupported;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SharingErrorCode {
    PortInUse,
    NoNetwork,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharingStatus {
    pub enabled: bool,
    pub running: bool,
    pub url: Option<String>,
    pub error: Option<String>,
    pub error_code: Option<SharingErrorCode>,
}

#[cfg(desktop)]
pub async fn start_if_enabled(app: &AppHandle) {
    desktop::start_if_enabled(app).await
}

#[cfg(not(desktop))]
pub async fn start_if_enabled(app: &AppHandle) {
    unsupported::start_if_enabled(app).await
}

#[cfg(desktop)]
pub async fn stop(app: &AppHandle) {
    desktop::stop(app).await
}

#[cfg(not(desktop))]
pub async fn stop(app: &AppHandle) {
    unsupported::stop(app).await
}

#[cfg(desktop)]
pub async fn core_changed(app: &AppHandle, core: Option<&Arc<CoreContext>>) {
    desktop::core_changed(app, core).await
}

#[cfg(not(desktop))]
pub async fn core_changed(app: &AppHandle, core: Option<&Arc<CoreContext>>) {
    unsupported::core_changed(app, core).await
}

#[cfg(desktop)]
#[tauri::command]
pub fn get_sharing_status(app: AppHandle) -> Result<SharingStatus, ErrorResponse> {
    desktop::status(&app)
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn get_sharing_status(app: AppHandle) -> Result<SharingStatus, ErrorResponse> {
    unsupported::status(&app)
}

#[cfg(desktop)]
#[tauri::command]
pub async fn set_sharing(app: AppHandle, enabled: bool) -> Result<SharingStatus, ErrorResponse> {
    desktop::set_sharing(&app, enabled).await
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn set_sharing(app: AppHandle, enabled: bool) -> Result<SharingStatus, ErrorResponse> {
    unsupported::set_sharing(&app, enabled).await
}

#[cfg(desktop)]
#[tauri::command]
pub async fn reset_sharing_port(app: AppHandle) -> Result<SharingStatus, ErrorResponse> {
    desktop::reset_port(&app).await
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn reset_sharing_port(app: AppHandle) -> Result<SharingStatus, ErrorResponse> {
    unsupported::reset_port(&app).await
}

#[cfg(desktop)]
#[tauri::command]
pub async fn list_paired_devices(
    app: AppHandle,
) -> Result<Vec<pipedash_core::application::services::device_service::DeviceView>, ErrorResponse> {
    desktop::list_devices(&app).await
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn list_paired_devices(app: AppHandle) -> Result<Vec<serde_json::Value>, ErrorResponse> {
    unsupported::list_devices(&app).await
}

#[cfg(desktop)]
#[tauri::command]
pub async fn create_pairing_code(
    app: AppHandle,
) -> Result<pipedash_web::pairing::PairingResponse, ErrorResponse> {
    desktop::create_pairing_code(&app).await
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn create_pairing_code(app: AppHandle) -> Result<serde_json::Value, ErrorResponse> {
    unsupported::create_pairing_code(&app).await
}

#[cfg(desktop)]
#[tauri::command]
pub async fn remove_paired_device(app: AppHandle, id: String) -> Result<(), ErrorResponse> {
    desktop::remove_device(&app, &id).await
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn remove_paired_device(app: AppHandle, id: String) -> Result<(), ErrorResponse> {
    unsupported::remove_device(&app, &id).await
}
