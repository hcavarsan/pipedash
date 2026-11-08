use std::{
    future::Future,
    time::Duration,
};

use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchStatus,
    BuildWatchStopReason,
};
use tauri::{
    AppHandle,
    Manager,
    Runtime,
};
use tauri_plugin_pipedash_native::PipedashNativeExt;

use super::{
    error,
    session::{
        BuildWatchState,
        StopTarget,
    },
};
use crate::error::ErrorResponse;

const NATIVE_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn native_request<T>(
    future: impl Future<Output = tauri_plugin_pipedash_native::Result<T>>,
) -> Result<T, ErrorResponse> {
    tokio::time::timeout(NATIVE_TIMEOUT, future)
        .await
        .map_err(|_| error("The native build watch request timed out"))?
        .map_err(|failure| error(&failure.to_string()))
}

pub(super) fn is_watching(status: &BuildWatchStatus, session_id: &str) -> bool {
    status.supported
        && status.available
        && status.watch.as_ref().is_some_and(|watch| {
            watch.session_id == session_id
                && watch.monitoring == BuildWatchMonitoring::Watching
                && !watch.status.is_terminal()
        })
}

#[cfg(desktop)]
pub(super) fn unavailable_status(mut status: BuildWatchStatus, reason: &str) -> BuildWatchStatus {
    status.available = false;
    status.reason = Some(reason.to_string());
    status.watch = None;
    status
}

#[cfg(desktop)]
pub(super) fn desktop_status(app: &AppHandle, mut status: BuildWatchStatus) -> BuildWatchStatus {
    if status
        .watch
        .as_ref()
        .is_some_and(|watch| !crate::desktop::watch::owns_watch(app, watch))
    {
        status.watch = None;
        status.reason = None;
    }
    status
}

pub(super) async fn stop_native<R: Runtime>(
    app: &AppHandle<R>, session_id: &str, reason: BuildWatchStopReason,
) -> Result<BuildWatchStatus, ErrorResponse> {
    native_request(app.pipedash_native().stop_build_watch(session_id, reason)).await
}

pub(super) async fn stop_current(
    app: &AppHandle, reason: BuildWatchStopReason,
) -> Result<BuildWatchStatus, ErrorResponse> {
    let session_id = app.state::<BuildWatchState>().cancel(None);
    if let Some(session_id) = session_id {
        return stop_native(app, &session_id, reason).await;
    }
    stop_orphan(app, reason).await
}

async fn stop_orphan<R: Runtime>(
    app: &AppHandle<R>, reason: BuildWatchStopReason,
) -> Result<BuildWatchStatus, ErrorResponse> {
    let status = native_request(app.pipedash_native().build_watch_status()).await?;
    if let Some(watch) = &status.watch {
        if watch.monitoring == BuildWatchMonitoring::Watching
            && app.state::<BuildWatchState>().session_id().as_deref()
                != Some(watch.session_id.as_str())
        {
            return stop_native(app, &watch.session_id, reason).await;
        }
    }
    Ok(status)
}

pub(super) fn spawn_interruption<R: Runtime>(
    app: &AppHandle<R>, state: &BuildWatchState, session_id: Option<String>,
) {
    let target = session_id.map_or(StopTarget::Orphan, StopTarget::Session);
    let worker = app.clone();
    state.queue_interruption(target, move || {
        tauri::async_runtime::spawn(drain_interruptions(worker))
    });
}

async fn drain_interruptions<R: Runtime>(app: AppHandle<R>) {
    let Some(state) = app.try_state::<BuildWatchState>() else {
        return;
    };
    while let Some(target) = state.next_interruption() {
        let result = match target {
            StopTarget::Session(session_id) => {
                stop_native(&app, &session_id, BuildWatchStopReason::Interrupted).await
            }
            StopTarget::Orphan => stop_orphan(&app, BuildWatchStopReason::Interrupted).await,
        };
        if result.is_err() {
            tracing::warn!("Could not interrupt the native build watch");
        }
    }
}
