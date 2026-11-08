mod biometry;
pub(crate) mod commands;

use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Mutex,
    MutexGuard,
    PoisonError,
};

use pipedash_core::event::CoreEvent;
use serde::{
    Deserialize,
    Serialize,
};
use tauri::{
    AppHandle,
    Manager,
};
#[cfg(target_os = "android")]
use tauri_plugin_pipedash_native::PipedashNativeExt;

use crate::MaybeCoreContext;

pub const SUPPORTED: bool = cfg!(mobile);

const SETTINGS_FILE: &str = "app_lock.json";
const VAULT_PASSWORD_ENV: &str = "PIPEDASH_VAULT_PASSWORD";
const CANCEL_CODES: [&str; 3] = ["userCancel", "systemCancel", "appCancel"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppLockSettings {
    pub enabled: bool,
    pub lock_on_background: bool,
}

#[cfg_attr(not(mobile), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Biometry {
    None,
    TouchId,
    FaceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiometryInfo {
    pub biometry: Biometry,
    pub available: bool,
}

impl BiometryInfo {
    const NONE: Self = Self {
        biometry: Biometry::None,
        available: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AppLockStatus {
    pub supported: bool,
    pub biometry: Biometry,
    pub biometry_available: bool,
    pub enabled: bool,
    pub lock_on_background: bool,
    pub locked: bool,
}

impl AppLockStatus {
    pub fn new(
        supported: bool, info: BiometryInfo, settings: AppLockSettings, locked: bool,
    ) -> Self {
        if !supported {
            return Self {
                supported,
                biometry: Biometry::None,
                biometry_available: false,
                enabled: false,
                lock_on_background: false,
                locked,
            };
        }

        Self {
            supported,
            biometry: info.biometry,
            biometry_available: info.available,
            enabled: settings.enabled,
            lock_on_background: settings.lock_on_background,
            locked,
        }
    }
}

#[cfg_attr(not(mobile), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum AuthOutcome {
    Authenticated,
    Cancelled,
    Failed(String),
}

#[cfg_attr(not(mobile), allow(dead_code))]
fn classify_rejection(code: Option<&str>, message: &str) -> AuthOutcome {
    match code {
        Some(code) if CANCEL_CODES.contains(&code) => AuthOutcome::Cancelled,
        _ => AuthOutcome::Failed(message.to_string()),
    }
}

pub fn starts_locked(supported: bool, settings: AppLockSettings, setup_complete: bool) -> bool {
    supported && settings.enabled && setup_complete
}

#[cfg_attr(not(mobile), allow(dead_code))]
fn resume_should_lock(
    settings: AppLockSettings, locked: bool, prompts_in_flight: u32, core_loaded: bool,
) -> bool {
    settings.enabled
        && settings.lock_on_background
        && !locked
        && prompts_in_flight == 0
        && core_loaded
}

fn read_settings(path: &Path) -> AppLockSettings {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!("Ignoring unreadable app lock settings: {}", e);
            AppLockSettings::default()
        }),
        Err(_) => AppLockSettings::default(),
    }
}

fn write_settings(path: &Path, settings: AppLockSettings) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&settings)?)?;
    std::fs::rename(&tmp, path)
}

#[derive(Debug, Default)]
struct Inner {
    settings: AppLockSettings,
    locked: bool,
    prompts_in_flight: u32,
}

pub struct AppLockState {
    path: PathBuf,
    inner: Mutex<Inner>,
    transition: tokio::sync::Mutex<()>,
}

pub struct PromptGuard<'a>(&'a AppLockState);

impl Drop for PromptGuard<'_> {
    fn drop(&mut self) {
        let mut inner = self.0.inner();
        inner.prompts_in_flight = inner.prompts_in_flight.saturating_sub(1);
    }
}

impl AppLockState {
    pub fn load(app_data_dir: &Path, supported: bool, setup_complete: bool) -> Self {
        let path = app_data_dir.join(SETTINGS_FILE);
        let settings = read_settings(&path);
        Self {
            path,
            inner: Mutex::new(Inner {
                settings,
                locked: starts_locked(supported, settings, setup_complete),
                prompts_in_flight: 0,
            }),
            transition: tokio::sync::Mutex::new(()),
        }
    }

    fn clear_settings(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.inner().settings = AppLockSettings::default();
        Ok(())
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn settings(&self) -> AppLockSettings {
        self.inner().settings
    }

    pub fn locked(&self) -> bool {
        self.inner().locked
    }

    pub fn set_locked(&self, locked: bool) {
        self.inner().locked = locked;
    }

    pub async fn transition(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.transition.lock().await
    }

    pub fn begin_prompt(&self) -> PromptGuard<'_> {
        self.inner().prompts_in_flight += 1;
        PromptGuard(self)
    }

    fn save_settings(&self, settings: AppLockSettings) -> std::io::Result<()> {
        write_settings(&self.path, settings)?;
        self.inner().settings = settings;
        Ok(())
    }

    #[cfg_attr(not(mobile), allow(dead_code))]
    fn should_lock_on_resume(&self, core_loaded: bool) -> bool {
        let inner = self.inner();
        resume_should_lock(
            inner.settings,
            inner.locked,
            inner.prompts_in_flight,
            core_loaded,
        )
    }
}

pub async fn status(app: &AppHandle) -> AppLockStatus {
    let lock = app.state::<AppLockState>();
    AppLockStatus::new(
        SUPPORTED,
        biometry::info(app).await,
        lock.settings(),
        lock.locked(),
    )
}

pub async fn lock_session(app: &AppHandle) {
    let lock = app.state::<AppLockState>();
    let _transition = lock.transition().await;
    #[cfg(desktop)]
    {
        crate::desktop::projection::detach(app);
        crate::desktop::flush_menu(app).await;
    }

    let core = app.state::<MaybeCoreContext>().0.write().await.take();
    crate::core_boot::background::cancel_init(app).await;
    crate::host::core_changed(app, None).await;
    std::env::remove_var(VAULT_PASSWORD_ENV);
    lock.set_locked(true);
    #[cfg(mobile)]
    crate::build_watch::interrupt(app);

    if let Some(core) = core {
        core.event_bus.emit(CoreEvent::VaultLocked).await;
        core.shutdown().await;
    }

    tracing::info!("Session locked - core shut down and vault password cleared");
}

#[cfg_attr(not(mobile), allow(dead_code))]
async fn lock_on_resume(app: &AppHandle) {
    if app
        .try_state::<crate::remote::connection::RemoteConnection>()
        .is_some_and(|remote| remote.is_client())
    {
        return;
    }
    #[cfg(target_os = "android")]
    if app
        .pipedash_native()
        .resumed_in_app()
        .await
        .unwrap_or(false)
    {
        return;
    }
    let core_loaded = app.state::<MaybeCoreContext>().0.read().await.is_some();
    if app
        .state::<AppLockState>()
        .should_lock_on_resume(core_loaded)
    {
        lock_session(app).await;
    }
}

#[cfg(mobile)]
pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if matches!(event, tauri::WindowEvent::Resumed) {
        let app = window.app_handle().clone();
        tauri::async_runtime::spawn(async move { lock_on_resume(&app).await });
    }
}

#[cfg(test)]
mod tests;
