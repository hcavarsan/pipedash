mod monitor;
mod native;
pub(crate) mod session;

use pipedash_plugin_api::build_watch::BuildWatchStatus;
#[cfg(desktop)]
use pipedash_plugin_api::build_watch::BuildWatchStopReason;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
    State,
};
use tauri_plugin_pipedash_native::PipedashNativeExt;

use self::session::BuildWatchState;
use crate::error::ErrorResponse;

fn error(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_string(),
        details: None,
        code: None,
    }
}

pub fn interrupt<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<BuildWatchState>() else {
        return;
    };
    native::spawn_interruption(app, &state, state.cancel(None));
}

#[cfg(mobile)]
pub fn interrupt_provider<R: Runtime>(app: &AppHandle<R>, provider_id: i64) {
    let Some(state) = app.try_state::<BuildWatchState>() else {
        return;
    };
    if let Some(session_id) = state.provider_session_id(provider_id) {
        if let Some(session_id) = state.cancel(Some(&session_id)) {
            native::spawn_interruption(app, &state, Some(session_id));
        }
    }
}

#[cfg(desktop)]
pub fn interrupt_provider_generation<R: Runtime>(
    app: &AppHandle<R>, generation: u64, provider_id: i64,
) {
    let Some(state) = app.try_state::<BuildWatchState>() else {
        return;
    };
    if let Some(session_id) = state.provider_session_id_generation(generation, provider_id) {
        if let Some(session_id) = state.cancel(Some(&session_id)) {
            native::spawn_interruption(app, &state, Some(session_id));
        }
    }
}

#[cfg(desktop)]
pub async fn stop_matching(app: &AppHandle, session_id: &str) -> Result<(), ErrorResponse> {
    if app
        .state::<BuildWatchState>()
        .cancel(Some(session_id))
        .is_some()
    {
        native::stop_native(app, session_id, BuildWatchStopReason::Stopped).await?;
    }
    Ok(())
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let handle = app.clone();
    app.pipedash_native()
        .on_build_watch_changed(move |status| {
            #[cfg(desktop)]
            if !crate::desktop::watch::watch_changed(&handle, &status) {
                return;
            }
            handle.state::<BuildWatchState>().native_changed(&status);
            if let Err(error) = handle.emit("build-watch-changed", status) {
                tracing::warn!(%error, "Could not publish the native build watch status change");
            }
        })
        .map_err(|error| tauri::Error::Anyhow(error.into()))
}

#[tauri::command]
pub async fn get_build_watch(app: AppHandle) -> Result<BuildWatchStatus, ErrorResponse> {
    monitor::current_status(&app).await
}

#[tauri::command]
pub async fn start_build_watch(
    app: AppHandle, state: State<'_, BuildWatchState>, pipeline_id: String, run_number: i64,
) -> Result<BuildWatchStatus, ErrorResponse> {
    monitor::start(app, &state, pipeline_id, run_number).await
}

#[tauri::command]
pub async fn stop_build_watch(app: AppHandle) -> Result<BuildWatchStatus, ErrorResponse> {
    monitor::stop(&app).await
}

#[cfg(test)]
mod tests;
