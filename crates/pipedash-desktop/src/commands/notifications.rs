use pipedash_core::application::services::push_service::{
    PushService,
    RelaySettings,
};
use pipedash_core::domain::notification_center::{
    self,
    NotificationCenterChange,
    NotificationCenterSnapshot,
};
use tauri::State;

use crate::error::ErrorResponse;
use crate::MaybeCoreContext;

#[tauri::command]
pub fn update_notification_center(
    snapshot: NotificationCenterSnapshot, change: NotificationCenterChange,
) -> Result<NotificationCenterSnapshot, ErrorResponse> {
    notification_center::update_notification_center(snapshot, change).map_err(Into::into)
}

async fn push_service(maybe_core: &MaybeCoreContext) -> Result<PushService, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|error| ErrorResponse {
        error,
        details: None,
        code: None,
    })?;

    Ok(core
        .push_service()
        .with_default_relay(crate::push::relay_url()))
}

#[tauri::command]
pub async fn get_push_relay(
    maybe_core: State<'_, MaybeCoreContext>,
) -> Result<RelaySettings, ErrorResponse> {
    Ok(push_service(&maybe_core).await?.relay_settings().await?)
}

#[tauri::command]
pub async fn set_push_relay(
    maybe_core: State<'_, MaybeCoreContext>, enabled: bool,
) -> Result<RelaySettings, ErrorResponse> {
    Ok(push_service(&maybe_core)
        .await?
        .set_relay_enabled(enabled)
        .await?)
}
