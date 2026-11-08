use std::{
    collections::VecDeque,
    marker::PhantomData,
    path::Path,
    sync::{
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
    },
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchSnapshot,
    BuildWatchStatus,
    BuildWatchStopReason,
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
    Ok(PipedashNative {
        watch: Mutex::new(WatchState::default()),
        runtime: PhantomData,
    })
}

pub struct PipedashNative<R: Runtime> {
    watch: Mutex<WatchState>,
    runtime: PhantomData<fn() -> R>,
}

#[derive(Default)]
struct WatchState {
    watch: Option<BuildWatchSnapshot>,
    reason: Option<String>,
    cancelled_sessions: VecDeque<String>,
    handlers: Vec<Arc<dyn Fn(BuildWatchStatus) + Send + Sync>>,
}

impl WatchState {
    fn status(&self) -> BuildWatchStatus {
        BuildWatchStatus {
            supported: true,
            available: true,
            reason: self.reason.clone(),
            watch: self.watch.clone(),
        }
    }
}

impl<R: Runtime> PipedashNative<R> {
    fn watch_state(&self) -> MutexGuard<'_, WatchState> {
        self.watch.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn change_watch(
        &self, change: impl FnOnce(&mut WatchState) -> Result<bool>,
    ) -> Result<BuildWatchStatus> {
        let (status, handlers) = {
            let mut state = self.watch_state();
            if !change(&mut state)? {
                return Ok(state.status());
            }
            (state.status(), state.handlers.clone())
        };
        for handler in handlers {
            handler(status.clone());
        }
        Ok(status)
    }

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
        Ok(self.watch_state().status())
    }

    pub async fn start_build_watch(&self, watch: &BuildWatchSnapshot) -> Result<BuildWatchStatus> {
        watch.validate().map_err(Error::Native)?;
        if watch.monitoring != BuildWatchMonitoring::Watching || watch.status.is_terminal() {
            return Err(Error::Native(
                "Only a queued or running build can be watched".to_string(),
            ));
        }
        self.change_watch(|state| {
            if state.cancelled_sessions.contains(&watch.session_id) {
                return Err(Error::Native(
                    "This build watch was already stopped".to_string(),
                ));
            }
            if let Some(current) = &state.watch {
                if current.monitoring == BuildWatchMonitoring::Watching {
                    return Err(Error::Native(
                        "Stop the current build watch before starting another".to_string(),
                    ));
                }
                if current.session_id == watch.session_id {
                    return Err(Error::Native(
                        "This build watch already ended. Start a new watch session.".to_string(),
                    ));
                }
            }
            state.watch = Some(watch.clone());
            state.reason = None;
            Ok(true)
        })
    }

    pub async fn update_build_watch(&self, watch: &BuildWatchSnapshot) -> Result<BuildWatchStatus> {
        watch.validate().map_err(Error::Native)?;
        self.change_watch(|state| {
            let Some(current) = &state.watch else {
                return Ok(false);
            };
            if current.monitoring != BuildWatchMonitoring::Watching
                || current.session_id != watch.session_id
                || current.pipeline_id != watch.pipeline_id
                || current.run_number != watch.run_number
                || current.provider_type != watch.provider_type
                || current.url != watch.url
                || watch.updated_at < current.updated_at
            {
                return Ok(false);
            }
            state.watch = Some(watch.clone());
            state.reason = None;
            Ok(true)
        })
    }

    pub async fn stop_build_watch(
        &self, session_id: &str, reason: BuildWatchStopReason,
    ) -> Result<BuildWatchStatus> {
        if session_id.trim().is_empty() || session_id.len() > 128 {
            return Err(Error::Native("Invalid build watch session".to_string()));
        }
        self.change_watch(|state| {
            if !state.cancelled_sessions.iter().any(|id| id == session_id) {
                state.cancelled_sessions.push_back(session_id.to_string());
                if state.cancelled_sessions.len() > 32 {
                    state.cancelled_sessions.pop_front();
                }
            }
            let Some(current) = state.watch.as_mut().filter(|current| {
                current.session_id == session_id
                    && current.monitoring == BuildWatchMonitoring::Watching
            }) else {
                return Ok(false);
            };
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| {
                    i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
                });
            current.updated_at = current.updated_at.max(now);
            let (monitoring, message) = match reason {
                BuildWatchStopReason::Stopped => (
                    BuildWatchMonitoring::Stopped,
                    "Monitoring stopped. The build continues on its provider.",
                ),
                BuildWatchStopReason::Interrupted => (
                    BuildWatchMonitoring::Interrupted,
                    "Monitoring was interrupted. Open Pipedash to start watching again.",
                ),
            };
            current.monitoring = monitoring;
            state.reason = Some(message.to_string());
            Ok(true)
        })
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

    pub async fn open_auth_session(&self, _url: &str) -> Result<bool> {
        Err(Error::Unsupported)
    }

    pub async fn close_auth_session(&self) -> Result<()> {
        Ok(())
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
            reason: Some("Background notifications require the Pipedash mobile app".to_string()),
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
        &self, handler: F,
    ) -> Result<()> {
        self.watch_state().handlers.push(Arc::new(handler));
        Ok(())
    }

    pub fn on_status_bar_tap<F: Fn() + Send + Sync + 'static>(&self, _handler: F) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
