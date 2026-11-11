use std::path::Path;

use pipedash_core::application::services::storage_service::config::load_config;
use pipedash_core::application::services::storage_service::vault::{
    requires_vault_password,
    UnlockVaultResponse,
};
use tauri::{
    AppHandle,
    Manager,
    State,
};

use super::{
    biometry,
    status,
    AppLockSettings,
    AppLockState,
    AppLockStatus,
    AuthOutcome,
    SUPPORTED,
    VAULT_PASSWORD_ENV,
};
use crate::commands::vault::unlock_session;
use crate::error::ErrorResponse;
use crate::{
    credential,
    AppDataDir,
};

const KEYCHAIN_SERVICE: &str = "pipedash";
const VAULT_PASSWORD_ACCOUNT: &str = "app_lock_vault_password";
const UNLOCK_REASON: &str = "Unlock Pipedash";
const ENABLE_REASON: &str = "Turn on app lock for Pipedash";

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

fn delete_saved_vault_password() -> credential::Result<()> {
    match vault_password_entry().and_then(|entry| entry.delete_credential()) {
        Ok(()) | Err(credential::Error::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}

fn failure(message: &str) -> UnlockVaultResponse {
    UnlockVaultResponse::failed(message)
}

fn error(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_string(),
        details: None,
        code: None,
    }
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ErrorResponse> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| error(&format!("App lock task failed: {e}")))
}

pub async fn reset(app: &AppHandle) -> Result<(), ErrorResponse> {
    let owned = app.clone();
    blocking(move || {
        delete_saved_vault_password()
            .map_err(|e| error(&format!("Couldn't delete the saved vault password: {e}")))?;
        let lock = owned.state::<AppLockState>();
        lock.clear_settings()
            .map_err(|e| error(&format!("Couldn't delete the app lock settings: {e}")))
    })
    .await??;
    crate::native::sync_privacy_cover(app, AppLockSettings::default());
    Ok(())
}

pub(super) fn enable_confirmed(outcome: AuthOutcome) -> Result<bool, ErrorResponse> {
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
        biometry::authenticate(&app, UNLOCK_REASON).await
    };
    match outcome {
        AuthOutcome::Authenticated => {}
        AuthOutcome::Cancelled => return Ok(UnlockVaultResponse::cancelled()),
        AuthOutcome::Failed(message) => return Ok(failure(&message)),
    }

    let config = load_config(&app_data_dir.0).await?;
    let password = if requires_vault_password(&config).await {
        match blocking(read_saved_vault_password).await? {
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

async fn password_to_save(app_data_dir: &Path) -> Result<Option<String>, ErrorResponse> {
    let config = load_config(app_data_dir).await?;
    if !requires_vault_password(&config).await {
        return Ok(None);
    }
    std::env::var(VAULT_PASSWORD_ENV)
        .map(Some)
        .map_err(|_| error("Unlock the vault before turning on app lock"))
}

async fn save_vault_password(password: String) -> Result<(), ErrorResponse> {
    blocking(move || vault_password_entry().and_then(|entry| entry.set_password(&password)))
        .await?
        .map_err(|e| {
            tracing::warn!("Couldn't save the vault password: {}", e);
            error("Couldn't save the vault password in the Keychain")
        })
}

async fn persist_settings(
    app: &AppHandle, current: AppLockSettings, requested: AppLockSettings, disabling: bool,
    saved_password: bool,
) -> Result<(), ErrorResponse> {
    let app = app.clone();
    blocking(move || {
        let lock = app.state::<AppLockState>();
        if disabling {
            return disable_app_lock(&lock, current, requested, delete_saved_vault_password);
        }
        lock.save_settings(requested).map_err(|e| {
            if saved_password {
                if let Err(cleanup) = delete_saved_vault_password() {
                    tracing::warn!(%cleanup, "Couldn't delete the saved vault password after settings failure");
                }
            }
            tracing::error!("Couldn't save app lock settings: {}", e);
            error("Couldn't save the app lock settings")
        })
    })
    .await?
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
        password_to_save(&app_data_dir.0).await?
    } else {
        None
    };

    if enabling {
        let outcome = {
            let _prompt = lock.begin_prompt();
            biometry::authenticate(&app, ENABLE_REASON).await
        };
        if !enable_confirmed(outcome)? {
            return Ok(status(&app).await);
        }
    }

    if let Some(password) = &vault_password {
        save_vault_password(password.clone()).await?;
    }

    let requested = AppLockSettings {
        enabled,
        lock_on_background,
    };
    let saved_password = vault_password.is_some();
    persist_settings(&app, current, requested, disabling, saved_password).await?;

    crate::native::sync_privacy_cover(&app, requested);

    Ok(status(&app).await)
}

pub(super) fn disable_app_lock<E: std::fmt::Display>(
    lock: &AppLockState, current: AppLockSettings, requested: AppLockSettings,
    delete_password: impl FnOnce() -> Result<(), E>,
) -> Result<(), ErrorResponse> {
    lock.save_settings(requested).map_err(|e| {
        tracing::error!("Couldn't save app lock settings: {}", e);
        error("Couldn't save the app lock settings")
    })?;
    delete_password().map_err(|e| {
        if let Err(restore) = lock.save_settings(current) {
            tracing::error!(%restore, "Couldn't restore app lock settings after the vault password deletion failed");
        }
        error(&format!("Couldn't delete the saved vault password: {e}"))
    })
}
