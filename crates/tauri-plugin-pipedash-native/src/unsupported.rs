use std::{
    marker::PhantomData,
    path::Path,
};

use serde::de::DeserializeOwned;
use tauri::{
    plugin::PluginApi,
    AppHandle,
    Runtime,
};

use crate::{
    Accessibility,
    Appearance,
    BuildWatchSnapshot,
    BuildWatchStatus,
    BuildWatchStopReason,
    Error,
    HapticKind,
    LocalNotificationSettings,
    NotificationRefresh,
    NotificationResponse,
    PrivacyCover,
    Result,
    ShareRect,
};

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>, _api: PluginApi<R, C>,
) -> Result<PipedashNative<R>> {
    Ok(PipedashNative(PhantomData))
}

pub struct PipedashNative<R: Runtime>(PhantomData<fn() -> R>);

impl<R: Runtime> PipedashNative<R> {
    pub async fn haptic(&self, _kind: HapticKind) -> Result<()> {
        Ok(())
    }

    pub async fn set_appearance(&self, _scheme: Appearance) -> Result<()> {
        Ok(())
    }

    pub async fn accessibility(&self) -> Result<Accessibility> {
        Ok(Accessibility::default())
    }

    pub async fn build_watch_status(&self) -> Result<BuildWatchStatus> {
        Ok(BuildWatchStatus {
            supported: false,
            available: false,
            reason: Some(
                "Build watching requires a physical iOS device running iOS 26 or later".to_string(),
            ),
            watch: None,
        })
    }

    pub async fn start_build_watch(&self, _watch: &BuildWatchSnapshot) -> Result<BuildWatchStatus> {
        Err(Error::Unsupported)
    }

    pub async fn update_build_watch(
        &self, _watch: &BuildWatchSnapshot,
    ) -> Result<BuildWatchStatus> {
        Err(Error::Unsupported)
    }

    pub async fn stop_build_watch(
        &self, _session_id: &str, _reason: BuildWatchStopReason,
    ) -> Result<BuildWatchStatus> {
        Err(Error::Unsupported)
    }

    pub async fn open_settings(&self) -> Result<()> {
        Err(Error::Unsupported)
    }

    pub async fn set_badge(&self, _count: u32) -> Result<()> {
        Ok(())
    }

    pub async fn configure_privacy_cover(&self, _cover: PrivacyCover) -> Result<()> {
        Ok(())
    }

    pub async fn release_privacy_cover(&self) -> Result<()> {
        Ok(())
    }

    pub async fn share_file(&self, _path: &Path) -> Result<bool> {
        Err(Error::Unsupported)
    }

    pub async fn share_link(&self, _url: &str, _source_rect: ShareRect) -> Result<bool> {
        Err(Error::Unsupported)
    }

    pub fn on_notification_response<F: Fn(NotificationResponse) + Send + Sync + 'static>(
        &self, _handler: F,
    ) -> Result<()> {
        Ok(())
    }

    pub async fn local_notification_settings(&self) -> Result<LocalNotificationSettings> {
        Ok(LocalNotificationSettings {
            enabled: false,
            available: false,
            reason: Some("Background notifications require the Pipedash iOS app".to_string()),
        })
    }

    pub async fn configure_local_notifications(
        &self, enabled: bool,
    ) -> Result<LocalNotificationSettings> {
        if enabled {
            return Err(Error::Unsupported);
        }
        self.local_notification_settings().await
    }

    pub async fn notification_refresh_ready(&self) -> Result<()> {
        Ok(())
    }

    pub async fn finish_notification_refresh(&self, _id: &str, _success: bool) -> Result<()> {
        Ok(())
    }

    pub fn on_notification_refresh<F: Fn(NotificationRefresh) + Send + Sync + 'static>(
        &self, _handler: F,
    ) -> Result<()> {
        Ok(())
    }

    pub fn on_build_watch_changed<F: Fn(BuildWatchStatus) + Send + Sync + 'static>(
        &self, _handler: F,
    ) -> Result<()> {
        Ok(())
    }

    pub fn on_status_bar_tap<F: Fn() + Send + Sync + 'static>(&self, _handler: F) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native() -> PipedashNative<tauri::Wry> {
        PipedashNative(PhantomData)
    }

    #[test]
    fn unsupported_platforms_report_notifications_off_and_unavailable() {
        let settings =
            tauri::async_runtime::block_on(native().local_notification_settings()).unwrap();
        assert!(!settings.enabled);
        assert!(!settings.available);
        assert!(settings.reason.is_some());
    }

    #[test]
    fn unsupported_platforms_reject_enabling_but_allow_idempotent_disabling() {
        let native = native();
        assert!(matches!(
            tauri::async_runtime::block_on(native.configure_local_notifications(true)),
            Err(Error::Unsupported)
        ));
        for _ in 0..2 {
            let settings =
                tauri::async_runtime::block_on(native.configure_local_notifications(false))
                    .unwrap();
            assert!(!settings.enabled);
            assert!(!settings.available);
        }
    }

    #[test]
    fn unsupported_platforms_accept_readiness_and_completion_without_a_task() {
        let native = native();
        assert!(tauri::async_runtime::block_on(native.notification_refresh_ready()).is_ok());
        assert!(
            tauri::async_runtime::block_on(native.finish_notification_refresh("late", false))
                .is_ok()
        );
        assert!(native.on_notification_refresh(|_| {}).is_ok());
    }
}
