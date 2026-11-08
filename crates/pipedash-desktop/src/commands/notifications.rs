use pipedash_core::domain::notification_center::{
    self,
    NotificationCenterChange,
    NotificationCenterSnapshot,
};

use crate::error::ErrorResponse;

#[tauri::command]
pub fn update_notification_center(
    snapshot: NotificationCenterSnapshot, change: NotificationCenterChange,
) -> Result<NotificationCenterSnapshot, ErrorResponse> {
    notification_center::update_notification_center(snapshot, change).map_err(Into::into)
}
