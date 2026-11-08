use tauri::AppHandle;
use tauri_plugin_pipedash_push::PipedashPushExt;

use crate::local_notifications::settings::{
    current_alert_host,
    mark_alert_host_push,
    notifications_enabled,
    set_push_problem,
};
use crate::local_notifications::store::AlertHost;
use crate::push::{
    relay,
    PushError,
    Result,
};

pub(in crate::push) trait Env {
    fn relay_url(&self) -> &str;
    async fn notifications_enabled(&self) -> bool;
    async fn stored_host(&self) -> Option<AlertHost>;
    async fn available(&self) -> bool;
    async fn permission_granted(&self) -> bool;
    async fn device_token(&self) -> Result<String>;
    async fn mark_push(&self, host: &AlertHost, push: bool);
    fn set_problem(&self, problem: Option<String>);
}

pub(in crate::push) struct AppEnv<'a>(pub &'a AppHandle);

impl Env for AppEnv<'_> {
    fn relay_url(&self) -> &str {
        relay::base_url()
    }

    async fn notifications_enabled(&self) -> bool {
        notifications_enabled(self.0).await
    }

    async fn stored_host(&self) -> Option<AlertHost> {
        current_alert_host().await
    }

    async fn available(&self) -> bool {
        cfg!(target_os = "ios") && matches!(self.0.pipedash_push().is_enabled().await, Ok(true))
    }

    async fn permission_granted(&self) -> bool {
        permission_granted(self.0).await
    }

    async fn device_token(&self) -> Result<String> {
        acquire_device_token(self.0).await
    }

    async fn mark_push(&self, host: &AlertHost, push: bool) {
        mark_alert_host_push(host, push).await;
    }

    fn set_problem(&self, problem: Option<String>) {
        set_push_problem(self.0, problem);
    }
}

#[cfg(target_os = "ios")]
async fn permission_granted(app: &AppHandle) -> bool {
    use tauri_plugin_notification::NotificationExt;

    let app = app.clone();
    matches!(
        tauri::async_runtime::spawn_blocking(move || app.notification().permission_state()).await,
        Ok(Ok(tauri::plugin::PermissionState::Granted))
    )
}

#[cfg(not(target_os = "ios"))]
async fn permission_granted(_app: &AppHandle) -> bool {
    false
}

#[cfg(target_os = "ios")]
async fn acquire_device_token(app: &AppHandle) -> Result<String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.pipedash_push().register_for_remote_notifications()
    })
    .await
    .map_err(|_| PushError::Registration)?
    .map_err(PushError::from)
}

#[cfg(not(target_os = "ios"))]
async fn acquire_device_token(_app: &AppHandle) -> Result<String> {
    Err(PushError::Unsupported)
}
