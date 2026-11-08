use std::sync::Arc;

use pipedash_core::CoreContext;
use serde_json::Value;
use tauri::AppHandle;

use super::SharingStatus;
use crate::error::ErrorResponse;

fn unsupported<T>() -> Result<T, ErrorResponse> {
    Err(ErrorResponse {
        error: "Sharing is available only on computers.".to_owned(),
        details: None,
        code: None,
    })
}

pub(super) async fn start_if_enabled(_app: &AppHandle) {}

pub(super) async fn stop(_app: &AppHandle) {}

pub(super) async fn core_changed(_app: &AppHandle, _core: Option<&Arc<CoreContext>>) {}

pub(super) fn status(_app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
    unsupported()
}

pub(super) async fn set_sharing(
    _app: &AppHandle, _enabled: bool,
) -> Result<SharingStatus, ErrorResponse> {
    unsupported()
}

pub(super) async fn reset_port(_app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
    unsupported()
}

pub(super) async fn list_devices(_app: &AppHandle) -> Result<Vec<Value>, ErrorResponse> {
    unsupported()
}

pub(super) async fn create_pairing_code(_app: &AppHandle) -> Result<Value, ErrorResponse> {
    unsupported()
}

pub(super) async fn remove_device(_app: &AppHandle, _id: &str) -> Result<(), ErrorResponse> {
    unsupported()
}
