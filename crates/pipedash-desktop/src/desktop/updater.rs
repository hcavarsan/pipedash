use std::{
    sync::{
        Mutex,
        MutexGuard,
        PoisonError,
    },
    time::Duration,
};

use serde::Serialize;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Url,
};
use tauri_plugin_updater::{
    Update,
    UpdaterExt,
};

use super::state::{
    failure,
    DesktopState,
};
use crate::error::ErrorResponse;

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const STARTUP_GRACE: Duration = Duration::from_secs(10);
const PROGRESS_STEP: u64 = 512 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Installing,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateStatus {
    pub phase: UpdatePhase,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub message: String,
}

impl UpdateStatus {
    fn step(&self, phase: UpdatePhase, message: String) -> Self {
        Self {
            phase,
            message,
            ..self.clone()
        }
    }

    fn target(&self) -> &str {
        self.version.as_deref().unwrap_or_default()
    }

    fn checking(&self) -> Self {
        self.step(UpdatePhase::Checking, "Checking for updates…".to_string())
    }

    fn up_to_date(&self) -> Self {
        Self {
            phase: UpdatePhase::UpToDate,
            message: format!("Pipedash {} is the latest version.", self.current_version),
            current_version: self.current_version.clone(),
            ..Default::default()
        }
    }

    fn available(&self, version: String, notes: Option<String>) -> Self {
        Self {
            phase: UpdatePhase::Available,
            message: format!("Pipedash {version} is available."),
            current_version: self.current_version.clone(),
            version: Some(version),
            notes,
            ..Default::default()
        }
    }

    fn check_failed(&self) -> Self {
        self.step(
            UpdatePhase::Failed,
            "Couldn't check for updates. Check your connection and retry.".to_string(),
        )
    }

    fn downloading(&self) -> Self {
        Self {
            downloaded: 0,
            total: None,
            ..self.step(
                UpdatePhase::Downloading,
                format!("Downloading Pipedash {}…", self.target()),
            )
        }
    }

    fn installing(&self) -> Self {
        self.step(
            UpdatePhase::Installing,
            format!(
                "Installing Pipedash {}. Pipedash restarts when it finishes.",
                self.target()
            ),
        )
    }

    fn install_failed(&self) -> Self {
        self.step(
            UpdatePhase::Failed,
            format!(
                "Couldn't install Pipedash {}. Retry, or download it from the releases page.",
                self.target()
            ),
        )
    }

    fn progress(&mut self, chunk: usize, total: Option<u64>) {
        self.downloaded += chunk as u64;
        self.total = total;
    }
}

fn should_auto_check(enabled: bool, phase: UpdatePhase) -> bool {
    enabled
        && matches!(
            phase,
            UpdatePhase::Idle | UpdatePhase::UpToDate | UpdatePhase::Failed
        )
}

#[derive(Default)]
struct Slot {
    status: UpdateStatus,
    pending: Option<Update>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
}

pub struct UpdaterState {
    slot: Mutex<Slot>,
    gate: tokio::sync::Mutex<()>,
    wake: tokio::sync::Notify,
}

impl UpdaterState {
    fn lock(&self) -> MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) fn manage(app: &AppHandle) {
    app.manage(UpdaterState {
        slot: Mutex::new(Slot {
            status: UpdateStatus {
                current_version: app.package_info().version.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }),
        gate: tokio::sync::Mutex::new(()),
        wake: tokio::sync::Notify::new(),
    });
    let task = tauri::async_runtime::spawn(auto_check(app.clone()));
    app.state::<UpdaterState>().lock().task = Some(task);
}

pub(super) fn stop(app: &AppHandle) {
    let task = app
        .try_state::<UpdaterState>()
        .and_then(|state| state.lock().task.take());
    if let Some(task) = task {
        task.abort();
    }
}

pub(super) fn wake(app: &AppHandle) {
    app.state::<UpdaterState>().wake.notify_one();
}

pub(super) fn core_attached(app: &AppHandle) {
    let state = app.state::<UpdaterState>();
    let phase = state.lock().status.phase;
    if matches!(phase, UpdatePhase::Idle | UpdatePhase::Failed) {
        state.wake.notify_one();
    }
}

fn publish(app: &AppHandle, next: impl FnOnce(&mut Slot) -> UpdateStatus) -> UpdateStatus {
    let status = {
        let state = app.state::<UpdaterState>();
        let mut slot = state.lock();
        let status = next(&mut slot);
        slot.status = status.clone();
        status
    };
    if let Err(error) = app.emit("update-status-changed", &status) {
        tracing::warn!(%error, "Couldn't publish the update status");
    }
    status
}

async fn proxy(app: &AppHandle) -> Option<Url> {
    let core = app.state::<crate::MaybeCoreContext>().get().await.ok()?;
    let config = core.config_state.get().await;
    pipedash_core::infrastructure::http_client::parse_proxy_url(&config.general.proxy_url)
        .ok()
        .flatten()
}

async fn find(app: &AppHandle) -> tauri_plugin_updater::Result<Option<Update>> {
    let mut builder = app.updater_builder().timeout(CHECK_TIMEOUT);
    if let Some(url) = proxy(app).await {
        builder = builder.proxy(url);
    }
    builder.build()?.check().await
}

async fn check(app: &AppHandle) -> UpdateStatus {
    let state = app.state::<UpdaterState>();
    let _gate = state.gate.lock().await;
    publish(app, |slot| slot.status.checking());
    let found = find(app).await;
    publish(app, |slot| match found {
        Ok(Some(update)) => {
            let status = slot
                .status
                .available(update.version.clone(), update.body.clone());
            slot.pending = Some(update);
            status
        }
        Ok(None) => {
            slot.pending = None;
            slot.status.up_to_date()
        }
        Err(error) => {
            tracing::warn!(%error, "Couldn't check for updates");
            slot.status.check_failed()
        }
    })
}

async fn auto_check(app: AppHandle) {
    let state = app.state::<UpdaterState>();
    let mut pause = STARTUP_GRACE;
    loop {
        tokio::select! {
            () = state.wake.notified() => {}
            () = tokio::time::sleep(pause) => {}
        }
        pause = CHECK_INTERVAL;
        let enabled = app
            .state::<DesktopState>()
            .lock()
            .saved
            .settings
            .auto_check_updates;
        let phase = state.lock().status.phase;
        if !cfg!(debug_assertions) && should_auto_check(enabled, phase) {
            check(&app).await;
        }
    }
}

#[tauri::command]
pub fn get_update_status(app: AppHandle) -> UpdateStatus {
    app.state::<UpdaterState>().lock().status.clone()
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> UpdateStatus {
    check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), ErrorResponse> {
    let state = app.state::<UpdaterState>();
    let _gate = state.gate.lock().await;
    let Some(update) = state.lock().pending.take() else {
        return Err(failure(
            "No update is ready to install. Check for updates first.",
        ));
    };
    publish(&app, |slot| slot.status.downloading());
    let mut published = 0;
    let installed = update
        .download_and_install(
            |chunk, total| {
                let downloaded = {
                    let mut slot = state.lock();
                    slot.status.progress(chunk, total);
                    slot.status.downloaded
                };
                if downloaded - published >= PROGRESS_STEP {
                    published = downloaded;
                    publish(&app, |slot| slot.status.clone());
                }
            },
            || {
                publish(&app, |slot| slot.status.installing());
            },
        )
        .await;
    if let Err(error) = installed {
        tracing::warn!(%error, "Couldn't install the update");
        let status = publish(&app, |slot| {
            slot.pending = Some(update);
            slot.status.install_failed()
        });
        return Err(failure(&status.message));
    }
    app.restart()
}

#[cfg(test)]
mod tests;
