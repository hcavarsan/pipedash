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
    State,
};

use crate::commands::{
    load_config_from_dir,
    unlock_session,
    vault_requires_password,
    ErrorResponse,
    UnlockVaultResponse,
};
use crate::{
    credential,
    AppDataDir,
    MaybeCoreContext,
};

pub const SUPPORTED: bool = cfg!(mobile);

const SETTINGS_FILE: &str = "app_lock.json";
const KEYCHAIN_SERVICE: &str = "pipedash";
const VAULT_PASSWORD_ACCOUNT: &str = "app_lock_vault_password";
const VAULT_PASSWORD_ENV: &str = "PIPEDASH_VAULT_PASSWORD";
const UNLOCK_REASON: &str = "Unlock Pipedash";
const ENABLE_REASON: &str = "Turn on app lock for Pipedash";
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

    fn clear_settings(&self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("Couldn't delete the app lock settings: {}", e),
        }
        self.inner().settings = AppLockSettings::default();
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

#[cfg(mobile)]
mod native {
    use tauri::AppHandle;
    use tauri_plugin_biometric::{
        AuthOptions,
        BiometricExt,
        BiometryType,
    };

    use super::{
        classify_rejection,
        AuthOutcome,
        Biometry,
        BiometryInfo,
    };

    pub async fn biometry(app: &AppHandle) -> BiometryInfo {
        let app = app.clone();
        match tauri::async_runtime::spawn_blocking(move || app.biometric().status()).await {
            Ok(Ok(status)) => BiometryInfo {
                biometry: match status.biometry_type {
                    BiometryType::None => Biometry::None,
                    BiometryType::TouchID => Biometry::TouchId,
                    BiometryType::FaceID => Biometry::FaceId,
                },
                available: status.is_available,
            },
            Ok(Err(e)) => {
                tracing::warn!("Couldn't read the biometric status: {}", e);
                BiometryInfo::NONE
            }
            Err(e) => {
                tracing::warn!("Biometric status task failed: {}", e);
                BiometryInfo::NONE
            }
        }
    }

    pub async fn authenticate(app: &AppHandle, reason: &str) -> AuthOutcome {
        let app = app.clone();
        let reason = reason.to_string();
        let options = AuthOptions {
            allow_device_credential: true,
            ..Default::default()
        };

        match tauri::async_runtime::spawn_blocking(move || {
            app.biometric().authenticate(reason, options)
        })
        .await
        {
            Ok(Ok(())) => AuthOutcome::Authenticated,
            Ok(Err(tauri_plugin_biometric::Error::PluginInvoke(
                tauri::plugin::mobile::PluginInvokeError::InvokeRejected(rejection),
            ))) => classify_rejection(
                rejection.code.as_deref(),
                rejection.message.as_deref().unwrap_or_default(),
            ),
            Ok(Err(e)) => AuthOutcome::Failed(e.to_string()),
            Err(e) => AuthOutcome::Failed(e.to_string()),
        }
    }
}

#[cfg(not(mobile))]
mod native {
    use tauri::AppHandle;

    use super::{
        AuthOutcome,
        BiometryInfo,
    };

    pub async fn biometry(_app: &AppHandle) -> BiometryInfo {
        BiometryInfo::NONE
    }

    pub async fn authenticate(_app: &AppHandle, _reason: &str) -> AuthOutcome {
        AuthOutcome::Failed("App lock is not supported on this platform".to_string())
    }
}

pub async fn status(app: &AppHandle) -> AppLockStatus {
    let lock = app.state::<AppLockState>();
    AppLockStatus::new(
        SUPPORTED,
        native::biometry(app).await,
        lock.settings(),
        lock.locked(),
    )
}

pub async fn lock_session(app: &AppHandle) {
    let lock = app.state::<AppLockState>();
    let _transition = lock.transition().await;

    let core = app.state::<MaybeCoreContext>().0.write().await.take();
    std::env::remove_var(VAULT_PASSWORD_ENV);
    lock.set_locked(true);

    if let Some(core) = core {
        core.event_bus.emit(CoreEvent::VaultLocked).await;
        core.shutdown().await;
    }

    tracing::info!("Session locked - core shut down and vault password cleared");
}

#[cfg_attr(not(mobile), allow(dead_code))]
async fn lock_on_resume(app: &AppHandle) {
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

fn vault_password_entry() -> credential::Result<credential::Entry> {
    credential::device_only_entry(KEYCHAIN_SERVICE, VAULT_PASSWORD_ACCOUNT)
}

fn read_saved_vault_password() -> credential::Result<Option<String>> {
    match vault_password_entry()?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(credential::Error::NoEntry) => Ok(None),
        Err(e) => Err(e),
    }
}

fn delete_saved_vault_password() {
    match vault_password_entry().and_then(|entry| entry.delete_credential()) {
        Ok(()) | Err(credential::Error::NoEntry) => {}
        Err(e) => tracing::warn!("Couldn't delete the saved vault password: {}", e),
    }
}

pub fn reset(lock: &AppLockState) {
    delete_saved_vault_password();
    lock.clear_settings();
}

fn failure(message: &str) -> UnlockVaultResponse {
    UnlockVaultResponse::failed(message)
}

fn error(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_string(),
        details: None,
    }
}

fn enable_confirmed(outcome: AuthOutcome) -> Result<bool, ErrorResponse> {
    match outcome {
        AuthOutcome::Authenticated => Ok(true),
        AuthOutcome::Cancelled => Ok(false),
        AuthOutcome::Failed(message) => Err(error(&message)),
    }
}

#[tauri::command]
pub async fn unlock_with_biometrics(
    app: AppHandle, lock: State<'_, AppLockState>, app_data_dir: State<'_, AppDataDir>,
) -> Result<UnlockVaultResponse, ErrorResponse> {
    if !SUPPORTED || !lock.settings().enabled {
        return Ok(failure("App lock is not enabled"));
    }
    if !lock.locked() {
        return Ok(UnlockVaultResponse::ok("Already unlocked"));
    }

    let outcome = {
        let _prompt = lock.begin_prompt();
        native::authenticate(&app, UNLOCK_REASON).await
    };
    match outcome {
        AuthOutcome::Authenticated => {}
        AuthOutcome::Cancelled => return Ok(UnlockVaultResponse::cancelled()),
        AuthOutcome::Failed(message) => return Ok(failure(&message)),
    }

    let config = load_config_from_dir(&app_data_dir.0)?;
    let password = if vault_requires_password(&config).await {
        match read_saved_vault_password() {
            Ok(Some(password)) => password,
            Ok(None) => {
                return Ok(failure(
                    "No saved vault password. Turn app lock off and on again.",
                ))
            }
            Err(e) => {
                tracing::warn!("Couldn't read the saved vault password: {}", e);
                return Ok(failure("Couldn't read the saved vault password"));
            }
        }
    } else {
        String::new()
    };

    unlock_session(&app, &password, true).await
}

#[tauri::command]
pub async fn set_app_lock(
    app: AppHandle, lock: State<'_, AppLockState>, app_data_dir: State<'_, AppDataDir>,
    enabled: bool, lock_on_background: bool,
) -> Result<AppLockStatus, ErrorResponse> {
    if !SUPPORTED {
        return Err(error("App lock is not supported on this device"));
    }
    if lock.locked() {
        return Err(error("Unlock Pipedash before changing app lock"));
    }

    let current = lock.settings();
    let enabling = enabled && !current.enabled;
    let disabling = !enabled && current.enabled;

    let vault_password = if enabling {
        let config = load_config_from_dir(&app_data_dir.0)?;
        if vault_requires_password(&config).await {
            Some(
                std::env::var(VAULT_PASSWORD_ENV)
                    .map_err(|_| error("Unlock the vault before turning on app lock"))?,
            )
        } else {
            None
        }
    } else {
        None
    };

    if enabling {
        let outcome = {
            let _prompt = lock.begin_prompt();
            native::authenticate(&app, ENABLE_REASON).await
        };
        if !enable_confirmed(outcome)? {
            return Ok(status(&app).await);
        }
    }

    if let Some(password) = &vault_password {
        vault_password_entry()
            .and_then(|entry| entry.set_password(password))
            .map_err(|e| {
                tracing::warn!("Couldn't save the vault password: {}", e);
                error("Couldn't save the vault password in the Keychain")
            })?;
    }

    let requested = AppLockSettings {
        enabled,
        lock_on_background,
    };
    if let Err(e) = lock.save_settings(requested) {
        if vault_password.is_some() {
            delete_saved_vault_password();
        }
        tracing::error!("Couldn't save app lock settings: {}", e);
        return Err(error("Couldn't save the app lock settings"));
    }

    if disabling {
        delete_saved_vault_password();
    }

    Ok(status(&app).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: AppLockSettings = AppLockSettings {
        enabled: true,
        lock_on_background: true,
    };

    #[test]
    fn the_app_starts_locked_only_when_supported_enabled_and_set_up() {
        assert!(starts_locked(true, ON, true));
        assert!(!starts_locked(false, ON, true));
        assert!(!starts_locked(true, AppLockSettings::default(), true));
        assert!(!starts_locked(true, ON, false));
    }

    #[test]
    fn resuming_locks_only_a_loaded_idle_core_with_background_lock_on() {
        assert!(resume_should_lock(ON, false, 0, true));
        assert!(!resume_should_lock(ON, true, 0, true));
        assert!(!resume_should_lock(ON, false, 1, true));
        assert!(!resume_should_lock(ON, false, 0, false));
        let foreground_only = AppLockSettings {
            enabled: true,
            lock_on_background: false,
        };
        assert!(!resume_should_lock(foreground_only, false, 0, true));
        assert!(!resume_should_lock(
            AppLockSettings::default(),
            false,
            0,
            true
        ));
    }

    #[test]
    fn a_biometric_prompt_blocks_locking_until_it_ends() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppLockState::load(dir.path(), true, true);
        state.save_settings(ON).unwrap();
        state.set_locked(false);

        let prompt = state.begin_prompt();
        let second = state.begin_prompt();
        assert!(!state.should_lock_on_resume(true));
        drop(prompt);
        assert!(!state.should_lock_on_resume(true));
        drop(second);
        assert!(state.should_lock_on_resume(true));
    }

    #[test]
    fn settings_survive_a_reload_and_bad_files_fall_back_to_off() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppLockState::load(dir.path(), true, true);
        assert!(!state.locked());
        state.save_settings(ON).unwrap();

        let reloaded = AppLockState::load(dir.path(), true, true);
        assert_eq!(reloaded.settings(), ON);
        assert!(reloaded.locked());

        std::fs::write(dir.path().join(SETTINGS_FILE), b"{ not json").unwrap();
        assert_eq!(
            read_settings(&dir.path().join(SETTINGS_FILE)),
            AppLockSettings::default()
        );
    }

    #[test]
    fn unsupported_platforms_report_no_app_lock_but_keep_the_session_flag() {
        let info = BiometryInfo {
            biometry: Biometry::FaceId,
            available: true,
        };
        let status = AppLockStatus::new(false, info, ON, true);
        assert_eq!(
            status,
            AppLockStatus {
                supported: false,
                biometry: Biometry::None,
                biometry_available: false,
                enabled: false,
                lock_on_background: false,
                locked: true,
            }
        );
    }

    #[test]
    fn supported_platforms_report_settings_and_biometry() {
        let info = BiometryInfo {
            biometry: Biometry::FaceId,
            available: true,
        };
        let status = AppLockStatus::new(true, info, ON, false);
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({
                "supported": true,
                "biometry": "faceId",
                "biometry_available": true,
                "enabled": true,
                "lock_on_background": true,
                "locked": false,
            })
        );
        assert_eq!(
            serde_json::to_value(Biometry::TouchId).unwrap(),
            serde_json::json!("touchId")
        );
    }

    #[test]
    fn only_a_cancelled_unlock_carries_a_reason() {
        assert_eq!(
            serde_json::to_value(UnlockVaultResponse::cancelled()).unwrap(),
            serde_json::json!({ "success": false, "message": "cancelled", "reason": "cancelled" })
        );
        assert_eq!(
            serde_json::to_value(UnlockVaultResponse::failed("nope")).unwrap(),
            serde_json::json!({ "success": false, "message": "nope" })
        );
    }

    #[test]
    fn cancel_codes_are_told_apart_from_real_failures() {
        for code in CANCEL_CODES {
            assert_eq!(classify_rejection(Some(code), "x"), AuthOutcome::Cancelled);
        }
        assert_eq!(
            classify_rejection(Some("biometryLockout"), "Locked out"),
            AuthOutcome::Failed("Locked out".to_string())
        );
        assert_eq!(
            classify_rejection(None, "boom"),
            AuthOutcome::Failed("boom".to_string())
        );
    }

    #[test]
    fn cancelling_the_enable_prompt_is_not_an_error() {
        assert!(matches!(
            enable_confirmed(AuthOutcome::Authenticated),
            Ok(true)
        ));
        assert!(matches!(
            enable_confirmed(AuthOutcome::Cancelled),
            Ok(false)
        ));
        let failed = enable_confirmed(AuthOutcome::Failed("Locked out".to_string()));
        assert!(matches!(failed, Err(ErrorResponse { error, .. }) if error == "Locked out"));
    }
}
