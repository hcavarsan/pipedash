mod menu;
pub mod notifications;
mod settings;

use std::{
    path::PathBuf,
    sync::{
        atomic::{
            AtomicU64,
            Ordering,
        },
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
    },
};

use pipedash_core::{
    application::services::health_service::NotificationState,
    domain::PipelineMenuSnapshot,
    event::CoreEvent,
    CoreContext,
};
use serde::Serialize;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
};
use tauri_plugin_pipedash_native::{
    BuildWatchMonitoring,
    BuildWatchSnapshot,
    BuildWatchStatus,
};

pub use self::settings::{
    DesktopSettings,
    DesktopSettingsPatch,
};
use self::{
    notifications::{
        DesktopNotice,
        DesktopNotificationSnapshot,
    },
    settings::SavedSettings,
};
use crate::{
    commands::ErrorResponse,
    native::NotificationTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ticket {
    generation: u64,
    epoch: u64,
}

#[derive(Clone, Serialize)]
pub struct DesktopStatus {
    revision: u64,
    settings: DesktopSettings,
    tray_state: &'static str,
    ready: bool,
    notification_preference_imported: bool,
    error: Option<String>,
}

struct Inner {
    saved: SavedSettings,
    revision: u64,
    generation: u64,
    epoch: u64,
    vault_ready: bool,
    core: Option<Arc<CoreContext>>,
    snapshot: Option<PipelineMenuSnapshot>,
    baseline: Option<NotificationState>,
    change_revision: u64,
    settings_error: Option<String>,
    data_error: Option<String>,
    tray_error: Option<String>,
    tray_ready: bool,
    refresh_active: bool,
    refresh_message: Option<String>,
    watch_session: Option<(String, Ticket)>,
    watch: Option<BuildWatchSnapshot>,
    history: DesktopNotificationSnapshot,
}

impl Inner {
    fn ticket(&self) -> Ticket {
        Ticket {
            generation: self.generation,
            epoch: self.epoch,
        }
    }
    fn matches(&self, ticket: Ticket) -> bool {
        self.vault_ready && self.core.is_some() && self.ticket() == ticket
    }
    fn status(&self) -> DesktopStatus {
        DesktopStatus {
            revision: self.revision,
            settings: self.saved.settings,
            tray_state: if self.tray_error.is_some() {
                "error"
            } else if self.tray_ready {
                "ready"
            } else {
                "off"
            },
            ready: self.vault_ready,
            notification_preference_imported: self.saved.notification_preference_imported,
            error: self
                .settings_error
                .as_ref()
                .or(self.tray_error.as_ref())
                .or(self.data_error.as_ref())
                .cloned(),
        }
    }
}

pub struct DesktopState {
    inner: Mutex<Inner>,
    path: PathBuf,
    settings_gate: tokio::sync::Mutex<()>,
    projection_gate: tokio::sync::Mutex<()>,
    refresh: tokio::sync::Notify,
    generations: AtomicU64,
    render: Box<dyn Fn() + Send + Sync>,
}

impl DesktopState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) fn failure(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_string(),
        details: None,
        code: None,
    }
}

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

fn publish_status<R: Runtime>(app: &AppHandle<R>) {
    let status = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.revision += 1;
        inner.status()
    };
    if let Err(error) = app.emit("desktop-status-changed", status) {
        tracing::warn!(%error, "Couldn't publish desktop integration status");
    }
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
            revision: 0,
            generation: 0,
            epoch: 0,
            vault_ready: false,
            core: None,
            snapshot: None,
            baseline: None,
            change_revision: 0,
            settings_error,
            data_error: None,
            tray_error: None,
            tray_ready: false,
            refresh_active: false,
            refresh_message: None,
            watch_session: None,
            watch: None,
            history: DesktopNotificationSnapshot::default(),
        }),
        path: file,
        settings_gate: tokio::sync::Mutex::new(()),
        refresh: tokio::sync::Notify::new(),
        projection_gate: tokio::sync::Mutex::new(()),
        generations: AtomicU64::new(0),
        render: Box::new(move || menu::render(&renderer)),
    });
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
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            handle.state::<DesktopState>().refresh.notified().await;
            refresh_projection(&handle).await;
        }
    });
    wake(app);
    Ok(())
}

pub fn wake<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<DesktopState>() {
        state.refresh.notify_one();
    }
}

pub async fn attach(app: &AppHandle, core: Arc<CoreContext>, generation: u64, vault_ready: bool) {
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.epoch += 1;
        inner.generation = generation;
        inner.core = Some(core);
        inner.vault_ready = vault_ready;
        inner.snapshot = None;
        inner.baseline = None;
        inner.data_error = None;
        inner.watch = None;
        inner.watch_session = None;
        inner.history.clear();
    }
    let history = app.state::<DesktopState>().lock().history.clone();
    notifications::publish(app, &history);
    refresh_projection(app).await;
    publish_status(app);
}

pub fn detach<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let history = {
        let mut inner = state.lock();
        inner.epoch += 1;
        inner.vault_ready = false;
        inner.core = None;
        inner.snapshot = None;
        inner.baseline = None;
        inner.watch_session = None;
        inner.watch = None;
        inner.refresh_active = false;
        inner.refresh_message = None;
        inner.data_error = None;
        inner.history.clear();
        inner.history.clone()
    };
    crate::build_watch::interrupt(app);
    notifications::publish(app, &history);
    publish_status(app);
    menu::schedule_render(app);
}

async fn refresh_projection(app: &AppHandle) {
    let state = app.state::<DesktopState>();
    let _projection = state.projection_gate.lock().await;
    let input = {
        let state = app.state::<DesktopState>();
        let inner = state.lock();
        inner
            .core
            .as_ref()
            .filter(|_| inner.vault_ready)
            .map(|core| {
                (
                    Arc::clone(core),
                    inner.ticket(),
                    inner.baseline.is_none(),
                    inner.change_revision,
                )
            })
    };
    if let Some((core, ticket, needs_seed, revision)) = input {
        let result = async {
            let snapshot = core.pipeline_view_service.menu_snapshot().await?;
            let baseline = if needs_seed {
                let pipelines = core.pipeline_service.get_cached_pipelines(None).await?;
                let providers = core.provider_service.list_providers().await?;
                let mut baseline = NotificationState::default();
                baseline.seed(&pipelines, &providers);
                Some(baseline)
            } else {
                None
            };
            Ok::<_, pipedash_core::domain::DomainError>((snapshot, baseline))
        }
        .await;
        let state = app.state::<DesktopState>();
        {
            let mut inner = state.lock();
            if !inner.matches(ticket) {
                return;
            }
            match result {
                Ok((snapshot, baseline)) => {
                    inner.snapshot = Some(snapshot);
                    inner.data_error = None;
                    if let Some(baseline) = baseline {
                        if revision == inner.change_revision {
                            inner.baseline = Some(baseline);
                        } else {
                            state.refresh.notify_one();
                        }
                    }
                }
                Err(_) => {
                    inner.snapshot = None;
                    inner.baseline = None;
                    inner.data_error = Some("Couldn't load pinned pipelines or notification baseline. Retry in General settings.".to_string());
                }
            }
        }
        publish_status(app);
    }
    menu::render(app);
}

pub fn observe<R: Runtime>(app: &AppHandle<R>, generation: u64, event: &CoreEvent) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let (ticket, notices, history) = {
        let mut inner = state.lock();
        if generation != inner.generation || !inner.vault_ready || inner.core.is_none() {
            return;
        }
        if matches!(
            event,
            CoreEvent::PipelinesUpdated { .. }
                | CoreEvent::PipelinesFailed { .. }
                | CoreEvent::ProviderAdded { .. }
                | CoreEvent::ProviderUpdated { .. }
                | CoreEvent::ProviderStatusUpdated { .. }
                | CoreEvent::ProviderRemoved { .. }
                | CoreEvent::ProviderHealthAlert { .. }
        ) {
            inner.change_revision += 1;
        }
        let notices: Vec<DesktopNotice> = inner
            .baseline
            .as_mut()
            .map(|baseline| {
                baseline
                    .observe_event(event)
                    .into_iter()
                    .map(DesktopNotice::from)
                    .collect()
            })
            .unwrap_or_default();
        for notice in &notices {
            inner.history.push(notice.clone());
        }
        let history = (!notices.is_empty()).then(|| inner.history.clone());
        (inner.ticket(), notices, history)
    };
    if let Some(history) = history {
        notifications::publish(app, &history);
        notifications::deliver(app, ticket, &notices);
    }
    if matches!(
        event,
        CoreEvent::PipelineViewsChanged
            | CoreEvent::PipelinesUpdated { .. }
            | CoreEvent::PipelineStatusChanged { .. }
            | CoreEvent::PipelineCacheInvalidated { .. }
            | CoreEvent::ProvidersChanged
            | CoreEvent::ProviderAdded { .. }
            | CoreEvent::ProviderUpdated { .. }
            | CoreEvent::ProviderRemoved { .. }
            | CoreEvent::ProviderStatusUpdated { .. }
            | CoreEvent::MigrationComplete { success: true, .. }
    ) {
        wake(app);
    }
}

pub fn reserve_watch<R: Runtime>(
    app: &AppHandle<R>, session_id: &str,
) -> Result<(), ErrorResponse> {
    let state = app.state::<DesktopState>();
    let mut inner = state.lock();
    if !inner.vault_ready {
        return Err(failure("Unlock Pipedash before watching a run"));
    }
    inner.watch_session = Some((session_id.to_string(), inner.ticket()));
    Ok(())
}

pub fn owns_watch<R: Runtime>(app: &AppHandle<R>, watch: &BuildWatchSnapshot) -> bool {
    let state = app.state::<DesktopState>();
    let inner = state.lock();
    inner.vault_ready
        && inner
            .watch
            .as_ref()
            .is_some_and(|current| current.session_id == watch.session_id)
}

pub fn watch_changed<R: Runtime>(app: &AppHandle<R>, status: &BuildWatchStatus) -> bool {
    let Some(watch) = &status.watch else {
        return false;
    };
    let state = app.state::<DesktopState>();
    let (ticket, notice, history) = {
        let mut inner = state.lock();
        let Some((session, ticket)) = &inner.watch_session else {
            return false;
        };
        if session != &watch.session_id || !inner.matches(*ticket) {
            return false;
        }
        let ticket = *ticket;
        if inner
            .watch
            .as_ref()
            .filter(|old| old.session_id == watch.session_id)
            .is_some_and(|old| {
                watch.updated_at < old.updated_at
                    || old.monitoring != BuildWatchMonitoring::Watching
            })
        {
            return false;
        }
        let completed = watch.monitoring == BuildWatchMonitoring::Completed
            && inner
                .watch
                .as_ref()
                .filter(|old| old.session_id == watch.session_id)
                .is_none_or(|old| old.monitoring == BuildWatchMonitoring::Watching);
        let notice = completed.then(|| DesktopNotice::watched(watch));
        if let Some(notice) = &notice {
            inner.history.push(notice.clone());
        }
        inner.watch = Some(watch.clone());
        let history = notice.as_ref().map(|_| inner.history.clone());
        (ticket, notice, history)
    };
    if let (Some(notice), Some(history)) = (notice, history) {
        notifications::publish(app, &history);
        notifications::deliver(app, ticket, &[notice]);
    }
    menu::schedule_render(app);
    true
}

pub async fn flush_menu(app: &AppHandle) {
    if on_main(app, menu::apply).await.is_err() {
        app.state::<DesktopState>().lock().tray_error = Some("Couldn't redact the native menu. Open Pipedash and quit to protect private pipeline names.".to_string());
        let _ = on_main(app, menu::remove_tray).await;
        app.state::<DesktopState>().lock().tray_ready = false;
        publish_status(app);
    }
}

async fn persist(app: &AppHandle, saved: SavedSettings) -> Result<(), ErrorResponse> {
    let file = app.state::<DesktopState>().path.clone();
    tauri::async_runtime::spawn_blocking(move || settings::save(&file, saved))
        .await
        .map_err(|_| failure("Desktop settings save was interrupted"))?
        .map_err(|_| failure("Couldn't save desktop settings. Check file permissions and retry."))
}

async fn commit_settings(
    app: &AppHandle, next: SavedSettings,
) -> Result<DesktopStatus, ErrorResponse> {
    let (previous, was_ready, settings_error) = {
        let state = app.state::<DesktopState>();
        let inner = state.lock();
        (
            inner.saved,
            inner.tray_ready,
            inner.settings_error.is_some(),
        )
    };
    let created = next.settings.show_tray && !was_ready;
    if created {
        if let Err(error) = on_main(app, menu::create_tray).await {
            app.state::<DesktopState>().lock().tray_error = Some(error.error.clone());
            publish_status(app);
            return Err(error);
        }
    }
    if !next.settings.show_tray && was_ready {
        on_main(app, show_main_window).await?;
    }
    if next != previous || settings_error {
        if let Err(error) = persist(app, next).await {
            if created {
                let _ = on_main(app, menu::remove_tray).await;
            }
            app.state::<DesktopState>().lock().settings_error = Some(error.error.clone());
            publish_status(app);
            return Err(error);
        }
    }
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.saved = next;
        inner.settings_error = None;
        inner.tray_error = None;
        inner.tray_ready = next.settings.show_tray && (was_ready || created);
    }
    if !next.settings.show_tray && was_ready {
        if let Err(error) = on_main(app, menu::remove_tray).await {
            app.state::<DesktopState>().lock().tray_error = Some(error.error);
        }
    }
    publish_status(app);
    wake(app);
    Ok(app.state::<DesktopState>().lock().status())
}

#[tauri::command]
pub fn get_desktop_status(app: AppHandle) -> DesktopStatus {
    app.state::<DesktopState>().lock().status()
}

#[tauri::command]
pub async fn update_desktop_settings(
    app: AppHandle, patch: DesktopSettingsPatch,
) -> Result<DesktopStatus, ErrorResponse> {
    let state = app.state::<DesktopState>();
    let _guard = state.settings_gate.lock().await;
    let empty = patch.show_tray.is_none()
        && patch.keep_running_on_close.is_none()
        && patch.notifications_enabled.is_none();
    if empty && state.lock().settings_error.is_some() {
        let path = state.path.clone();
        let (saved, error) = tauri::async_runtime::spawn_blocking(move || settings::load(&path))
            .await
            .map_err(|_| failure("Couldn't reread desktop settings"))?;
        let failed = error.is_some();
        {
            let mut inner = state.lock();
            inner.settings_error = error;
            if !failed {
                inner.saved = saved;
            }
        }
        if failed {
            publish_status(&app);
            refresh_projection(&app).await;
            return Ok(state.lock().status());
        }
    }
    let next = state.lock().saved.patch(patch)?;
    commit_settings(&app, next).await?;
    refresh_projection(&app).await;
    Ok(app.state::<DesktopState>().lock().status())
}

#[tauri::command]
pub async fn import_desktop_notification_preference(
    app: AppHandle, enabled: bool,
) -> Result<DesktopStatus, ErrorResponse> {
    let state = app.state::<DesktopState>();
    let _guard = state.settings_gate.lock().await;
    if state.lock().settings_error.is_some() {
        return Ok(state.lock().status());
    }
    let mut next = state.lock().saved;
    if next.notification_preference_imported {
        return Ok(state.lock().status());
    }
    next.notification_preference_imported = true;
    next.settings.notifications_enabled = enabled;
    commit_settings(&app, next).await
}

pub async fn reset_settings(app: &AppHandle) -> Result<(), ErrorResponse> {
    let state = app.state::<DesktopState>();
    let _guard = state.settings_gate.lock().await;
    commit_settings(
        app,
        SavedSettings {
            notification_preference_imported: true,
            ..Default::default()
        },
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub fn test_desktop_notification(app: AppHandle) -> Result<(), ErrorResponse> {
    if !ready(&app) {
        return Err(failure(
            "Unlock and initialize Pipedash before sending a test",
        ));
    }
    if !app
        .state::<DesktopState>()
        .lock()
        .saved
        .settings
        .notifications_enabled
    {
        return Err(failure(
            "Turn on system notifications before sending a test",
        ));
    }
    crate::system_notifications::send(
        &app,
        "Pipedash notification test",
        "System notifications are configured for this Pipedash session.",
        Some(&NotificationTarget {
            key: Some("desktop-test".to_string()),
            ..Default::default()
        }),
    )
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
        tauri::RunEvent::Exit => detach(app),
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
    match id {
        "settings" => menu::navigate(app, "/settings/general", None),
        "lock" => menu::dispatch(app, menu::Command::Lock, None),
        "refresh-all" => menu::dispatch(app, menu::Command::Refresh, None),
        _ => {}
    }
}
