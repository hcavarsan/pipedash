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

use serde::de::DeserializeOwned;
use tauri::{
    plugin::PluginApi,
    AppHandle,
    Runtime,
};

use crate::{
    Accessibility,
    Appearance,
    BuildWatchMonitoring,
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
        watch.validate()?;
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
        watch.validate()?;
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
mod tests {
    use super::*;
    use crate::BuildWatchRunStatus;

    fn native() -> PipedashNative<tauri::Wry> {
        PipedashNative {
            watch: Mutex::new(WatchState::default()),
            runtime: PhantomData,
        }
    }

    fn snapshot(session_id: &str) -> BuildWatchSnapshot {
        BuildWatchSnapshot {
            session_id: session_id.to_string(),
            pipeline_id: "github__1__owner/repo/build.yml".to_string(),
            pipeline_name: "Build".to_string(),
            provider_name: "GitHub".to_string(),
            provider_type: "github".to_string(),
            run_number: 7,
            branch: Some("main".to_string()),
            status: BuildWatchRunStatus::Running,
            started_at: 100,
            concluded_at: None,
            updated_at: 110,
            monitoring: BuildWatchMonitoring::Watching,
            url: BuildWatchSnapshot::run_url("github__1__owner/repo/build.yml", 7),
        }
    }

    #[test]
    fn desktop_watches_are_available_without_restoring_an_old_session() {
        tauri::async_runtime::block_on(async {
            let first = native();
            let initial = first.build_watch_status().await.unwrap();
            assert!(initial.supported);
            assert!(initial.available);
            assert_eq!(initial.watch, None);
            first.start_build_watch(&snapshot("old")).await.unwrap();
            assert_eq!(native().build_watch_status().await.unwrap(), initial);
        });
    }

    #[test]
    fn start_rejects_invalid_inactive_and_overlapping_watches() {
        tauri::async_runtime::block_on(async {
            let native = native();
            let mut invalid = snapshot("invalid");
            invalid.pipeline_id.clear();
            assert!(native.start_build_watch(&invalid).await.is_err());
            for monitoring in [
                BuildWatchMonitoring::Completed,
                BuildWatchMonitoring::Stopped,
                BuildWatchMonitoring::Interrupted,
            ] {
                let mut ended = snapshot("ended");
                ended.monitoring = monitoring;
                ended.status = BuildWatchRunStatus::Success;
                assert!(native.start_build_watch(&ended).await.is_err());
            }
            let current = native
                .start_build_watch(&snapshot("current"))
                .await
                .unwrap();
            assert!(native.start_build_watch(&snapshot("second")).await.is_err());
            assert_eq!(native.build_watch_status().await.unwrap(), current);
        });
    }

    #[test]
    fn stop_before_start_blocks_the_cancelled_session() {
        tauri::async_runtime::block_on(async {
            let native = native();
            let stopped = native
                .stop_build_watch("cancelled", BuildWatchStopReason::Stopped)
                .await
                .unwrap();
            assert_eq!(stopped.watch, None);
            assert!(native
                .start_build_watch(&snapshot("cancelled"))
                .await
                .is_err());
            assert!(native
                .start_build_watch(&snapshot("replacement"))
                .await
                .is_ok());
        });
    }

    #[test]
    fn cancellation_tombstones_keep_the_last_32_distinct_sessions() {
        tauri::async_runtime::block_on(async {
            let native = native();
            for id in 0..32 {
                native
                    .stop_build_watch(&id.to_string(), BuildWatchStopReason::Stopped)
                    .await
                    .unwrap();
            }
            native
                .stop_build_watch("0", BuildWatchStopReason::Stopped)
                .await
                .unwrap();
            native
                .stop_build_watch("32", BuildWatchStopReason::Stopped)
                .await
                .unwrap();
            assert!(native.start_build_watch(&snapshot("1")).await.is_err());
            assert!(native.start_build_watch(&snapshot("32")).await.is_err());
            assert!(native.start_build_watch(&snapshot("0")).await.is_ok());
        });
    }

    #[test]
    fn stale_stops_cannot_stop_a_replacement_watch() {
        tauri::async_runtime::block_on(async {
            let native = native();
            native.start_build_watch(&snapshot("old")).await.unwrap();
            native
                .stop_build_watch("old", BuildWatchStopReason::Stopped)
                .await
                .unwrap();
            let replacement = native.start_build_watch(&snapshot("new")).await.unwrap();
            assert_eq!(
                native
                    .stop_build_watch("old", BuildWatchStopReason::Interrupted)
                    .await
                    .unwrap(),
                replacement,
            );
        });
    }

    #[test]
    fn updates_preserve_run_identity_and_observation_order() {
        tauri::async_runtime::block_on(async {
            let native = native();
            let watch = snapshot("current");
            let current = native.start_build_watch(&watch).await.unwrap();
            for changed_field in 0..5 {
                let mut changed = watch.clone();
                match changed_field {
                    0 => changed.session_id = "other".to_string(),
                    1 => changed.pipeline_id = "another-pipeline".to_string(),
                    2 => changed.run_number += 1,
                    3 => changed.provider_type = "gitea".to_string(),
                    _ => changed.updated_at -= 1,
                }
                changed.url = BuildWatchSnapshot::run_url(&changed.pipeline_id, changed.run_number);
                assert_eq!(native.update_build_watch(&changed).await.unwrap(), current);
            }
            let mut invalid = watch.clone();
            invalid.url = "https://example.com/private".to_string();
            assert!(native.update_build_watch(&invalid).await.is_err());
            let mut updated = watch;
            updated.pipeline_name = "Renamed".to_string();
            updated.provider_name = "Renamed provider".to_string();
            updated.branch = Some("release".to_string());
            assert_eq!(
                native.update_build_watch(&updated).await.unwrap().watch,
                Some(updated)
            );
        });
    }

    #[test]
    fn completed_watches_retain_the_terminal_result_until_replacement() {
        tauri::async_runtime::block_on(async {
            let native = native();
            let watch = snapshot("completed");
            native.start_build_watch(&watch).await.unwrap();
            let mut completed = watch.clone();
            completed.monitoring = BuildWatchMonitoring::Completed;
            completed.status = BuildWatchRunStatus::Failed;
            completed.concluded_at = Some(120);
            completed.updated_at = 120;
            let terminal = native.update_build_watch(&completed).await.unwrap();
            assert_eq!(terminal.watch, Some(completed));
            assert_eq!(native.update_build_watch(&watch).await.unwrap(), terminal);
            assert_eq!(
                native
                    .stop_build_watch("completed", BuildWatchStopReason::Stopped)
                    .await
                    .unwrap(),
                terminal,
            );
            assert_eq!(native.build_watch_status().await.unwrap(), terminal);
            assert!(native.start_build_watch(&watch).await.is_err());
            assert!(native.start_build_watch(&snapshot("new")).await.is_ok());
        });
    }

    #[test]
    fn stop_and_interruption_retain_snapshots_without_decreasing_time() {
        tauri::async_runtime::block_on(async {
            for reason in [
                BuildWatchStopReason::Stopped,
                BuildWatchStopReason::Interrupted,
            ] {
                let native = native();
                let mut watch = snapshot("current");
                watch.updated_at = i64::MAX;
                native.start_build_watch(&watch).await.unwrap();
                let ended = native.stop_build_watch("current", reason).await.unwrap();
                let retained = ended.watch.as_ref().unwrap();
                assert_eq!(retained.updated_at, i64::MAX);
                assert_eq!(retained.pipeline_id, watch.pipeline_id);
                assert_eq!(retained.status, watch.status);
                assert_eq!(retained.concluded_at, watch.concluded_at);
                assert_eq!(
                    retained.monitoring,
                    match reason {
                        BuildWatchStopReason::Stopped => BuildWatchMonitoring::Stopped,
                        BuildWatchStopReason::Interrupted => BuildWatchMonitoring::Interrupted,
                    }
                );
                assert!(ended.reason.is_some());
                assert_eq!(native.update_build_watch(&watch).await.unwrap(), ended);
                assert_eq!(native.build_watch_status().await.unwrap(), ended);
                assert!(native.start_build_watch(&watch).await.is_err());
                let replacement = native.start_build_watch(&snapshot("new")).await.unwrap();
                assert_eq!(replacement.reason, None);
            }
        });
    }

    #[test]
    fn callbacks_run_outside_the_mutex_only_for_accepted_changes() {
        tauri::async_runtime::block_on(async {
            let native = Arc::new(native());
            let weak = Arc::downgrade(&native);
            let received = Arc::new(Mutex::new(Vec::new()));
            let statuses = Arc::clone(&received);
            native
                .on_build_watch_changed(move |status| {
                    assert!(weak.upgrade().unwrap().watch.try_lock().is_ok());
                    statuses.lock().unwrap().push(status);
                })
                .unwrap();
            native
                .on_build_watch_changed(|status| assert!(status.supported))
                .unwrap();
            assert!(native
                .stop_build_watch("", BuildWatchStopReason::Stopped)
                .await
                .is_err());
            assert!(native
                .stop_build_watch(&"a".repeat(129), BuildWatchStopReason::Stopped)
                .await
                .is_err());
            native
                .stop_build_watch("absent", BuildWatchStopReason::Stopped)
                .await
                .unwrap();
            let mut watch = snapshot("current");
            native.start_build_watch(&watch).await.unwrap();
            assert!(native.start_build_watch(&snapshot("second")).await.is_err());
            watch.updated_at -= 1;
            native.update_build_watch(&watch).await.unwrap();
            watch.updated_at += 2;
            native.update_build_watch(&watch).await.unwrap();
            native
                .stop_build_watch("current", BuildWatchStopReason::Stopped)
                .await
                .unwrap();
            native
                .stop_build_watch("current", BuildWatchStopReason::Interrupted)
                .await
                .unwrap();
            let received = received.lock().unwrap();
            assert_eq!(received.len(), 3);
            assert_eq!(received[0].watch.as_ref().unwrap().updated_at, 110);
            assert_eq!(received[1].watch.as_ref().unwrap().updated_at, 111);
            assert_eq!(
                received[2].watch.as_ref().unwrap().monitoring,
                BuildWatchMonitoring::Stopped
            );
        });
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
