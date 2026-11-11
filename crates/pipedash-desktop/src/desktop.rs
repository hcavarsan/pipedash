mod menu;
pub mod notifications;
pub(crate) mod projection;
pub(crate) mod settings;
mod state;
#[cfg(test)]
mod tests;
mod tray;
pub(crate) mod updater;
pub(crate) mod watch;

use std::{
    path::PathBuf,
    sync::{
        atomic::{
            AtomicU64,
            Ordering,
        },
        Mutex,
    },
};

use tauri::{
    AppHandle,
    Manager,
    Runtime,
};

use self::state::{
    failure,
    publish_status,
    DesktopState,
    Inner,
};
use crate::error::ErrorResponse;

pub fn ready<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<DesktopState>()
        .is_some_and(|state| state.lock().vault_ready)
}

pub fn new_generation<R: Runtime>(app: &AppHandle<R>) -> u64 {
    app.state::<DesktopState>()
        .generations
        .fetch_add(1, Ordering::Relaxed)
        + 1
}

pub fn generation<R: Runtime>(app: &AppHandle<R>) -> u64 {
    app.state::<DesktopState>().lock().generation
}

pub fn tray_changed<R: Runtime>(app: &AppHandle<R>) {
    publish_status(app);
    menu::schedule_render(app);
}

pub fn fit_main_window_to_monitor<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or(tauri::Error::WindowNotFound)?;
    let monitor = window.current_monitor()?.or(window.primary_monitor()?);
    let Some(monitor) = monitor else {
        return Ok(());
    };
    if window_exceeds_work_area(window.outer_size()?, monitor.work_area().size) {
        window.maximize()?;
    }
    Ok(())
}

fn window_exceeds_work_area(
    window: tauri::PhysicalSize<u32>, work_area: tauri::PhysicalSize<u32>,
) -> bool {
    window.width > work_area.width || window.height > work_area.height
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or(tauri::Error::WindowNotFound)?;
    window.unminimize()?;
    window.show()?;
    window.set_focus()
}

async fn on_main<R: Runtime, T: Send + 'static>(
    app: &AppHandle<R>, work: impl FnOnce(&AppHandle<R>) -> tauri::Result<T> + Send + 'static,
) -> Result<T, ErrorResponse> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(work(&handle));
    })
    .map_err(|_| failure("Couldn't schedule the desktop update"))?;
    receiver
        .await
        .map_err(|_| failure("The desktop update was interrupted"))?
        .map_err(|_| failure("Couldn't update the native menu or tray"))
}

pub async fn initialize(app: &AppHandle, path: PathBuf) -> Result<(), ErrorResponse> {
    let file = path.join("desktop.json");
    let load = file.clone();
    let (saved, settings_error) =
        tauri::async_runtime::spawn_blocking(move || settings::load(&load))
            .await
            .map_err(|_| failure("Couldn't load desktop settings"))?;
    let renderer = app.clone();
    app.manage(DesktopState {
        inner: Mutex::new(Inner {
            saved,
            settings_error,
            ..Default::default()
        }),
        path: file,
        settings_gate: tokio::sync::Mutex::new(()),
        refresh: tokio::sync::Notify::new(),
        projection_gate: tokio::sync::Mutex::new(()),
        generations: AtomicU64::new(0),
        render: Box::new(move || menu::render(&renderer)),
    });
    updater::manage(app);
    menu::initialize(app).map_err(|_| failure("Couldn't initialize the native menu"))?;
    if saved.settings.show_tray {
        let result = menu::create_tray(app);
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.tray_ready = result.is_ok();
        if result.is_err() {
            inner.tray_error =
                Some("Couldn't create the system tray. Retry in General settings.".to_string());
        }
    }
    projection::start_tasks(app);
    projection::wake(app);
    Ok(())
}

pub async fn flush_menu(app: &AppHandle) {
    if on_main(app, menu::apply).await.is_err() {
        app.state::<DesktopState>().lock().tray_error = Some("Couldn't redact the native menu. Open Pipedash and quit to protect private pipeline names.".to_string());
        let _ = on_main(app, menu::remove_tray).await;
        app.state::<DesktopState>().lock().tray_ready = false;
        publish_status(app);
    }
}

pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let background = window
        .app_handle()
        .try_state::<DesktopState>()
        .is_some_and(|state| {
            let inner = state.lock();
            inner.saved.settings.keep_running_on_close
                && inner.tray_ready
                && inner.tray_error.is_none()
        });
    if !background {
        return;
    }
    #[cfg(target_os = "linux")]
    let result = window.minimize();
    #[cfg(not(target_os = "linux"))]
    let result = window.hide();
    match result {
        Ok(()) => api.prevent_close(),
        Err(error) => tracing::warn!(%error, "Couldn't retain the background window"),
    }
}

pub fn on_run_event(app: &AppHandle, event: &tauri::RunEvent) {
    match event {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            if let Err(error) = show_main_window(app) {
                tracing::warn!(%error, "Couldn't reopen Pipedash");
            }
        }
        tauri::RunEvent::Exit => {
            updater::stop(app);
            projection::shutdown(app);
        }
        _ => {}
    }
}

pub fn initialization_failed(app: &AppHandle, message: &str) {
    app.state::<DesktopState>().lock().data_error = Some(message.to_string());
    publish_status(app);
    menu::schedule_render(app);
}

#[cfg(target_os = "macos")]
pub fn dispatch_app_menu(app: &AppHandle, id: &str) {
    use self::menu::actions::{
        dispatch,
        navigate,
        Command,
    };

    match id {
        "settings" => navigate(app, "/settings/general", None),
        "lock" => dispatch(app, Command::Lock, None),
        "refresh-all" => dispatch(app, Command::Refresh, None),
        _ => {}
    }
}
