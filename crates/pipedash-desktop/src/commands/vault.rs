use pipedash_core::application::services::storage_service::config::load_config;
use pipedash_core::application::services::storage_service::vault::{
    requires_vault_password,
    vault_is_unlocked,
    vault_password_status,
    PasswordSource,
    UnlockVaultResponse,
    VaultPasswordStatus,
    VaultStatus,
};
use pipedash_core::infrastructure::migration::VAULT_PASSWORD_ENV_VAR;
use serde::Serialize;
use tauri::{
    Manager,
    State,
};

use crate::app_lock::{
    self,
    AppLockStatus,
};
use crate::core_boot::{
    boot_core,
    BootError,
    BootKind,
};
use crate::error::ErrorResponse;
use crate::AppDataDir;

#[derive(Debug, Serialize)]
pub struct VaultStatusResponse {
    #[serde(flatten)]
    pub status: VaultStatus,
    pub app_lock: AppLockStatus,
}

#[tauri::command]
pub async fn get_vault_password_status() -> VaultPasswordStatus {
    vault_password_status()
}

#[tauri::command]
pub async fn get_vault_status(
    app: tauri::AppHandle, maybe_core: State<'_, crate::MaybeCoreContext>,
    app_data_dir: State<'_, AppDataDir>,
) -> Result<VaultStatusResponse, ErrorResponse> {
    let config = load_config(&app_data_dir.0).await?;

    let requires_password = requires_vault_password(&config).await;

    let vault_password_set = std::env::var(VAULT_PASSWORD_ENV_VAR).is_ok();

    let password_source = if !requires_password {
        PasswordSource::Keyring
    } else if vault_password_set {
        PasswordSource::EnvVar
    } else {
        PasswordSource::None
    };

    let app_lock = app_lock::status(&app).await;

    let is_unlocked = vault_is_unlocked(requires_password, vault_password_set, app_lock.locked);

    let core = maybe_core.0.read().await.clone();
    let is_first_time = if let Some(core) = core {
        core.provider_service
            .list_providers()
            .await
            .map(|p| p.is_empty())
            .unwrap_or(true)
    } else {
        !app_lock.locked
    };

    tracing::debug!(
        "Vault status: requires_password={}, is_unlocked={}, locked={}, backend={}",
        requires_password,
        is_unlocked,
        app_lock.locked,
        config.storage.backend
    );

    Ok(VaultStatusResponse {
        status: VaultStatus {
            is_unlocked,
            password_source,
            backend: config.storage.backend.to_string(),
            requires_password,
            is_first_time,
        },
        app_lock,
    })
}

#[tauri::command]
pub async fn unlock_vault(
    app: tauri::AppHandle, password: String,
) -> Result<UnlockVaultResponse, ErrorResponse> {
    unlock_session(&app, &password, false).await
}

pub(crate) async fn unlock_session(
    app: &tauri::AppHandle, password: &str, authenticated: bool,
) -> Result<UnlockVaultResponse, ErrorResponse> {
    let data_dir = app.state::<AppDataDir>().0.clone();
    let config = load_config(&data_dir).await?;

    if !requires_vault_password(&config).await {
        return unlock_keyring_session(app, password, authenticated).await;
    }

    if password.is_empty() {
        return Ok(UnlockVaultResponse::failed(
            "Password required to unlock encrypted vault",
        ));
    }

    std::env::set_var(VAULT_PASSWORD_ENV_VAR, password);
    tracing::info!("Vault password set in environment for session");

    match boot_core(app, BootKind::Unlock, true).await {
        Ok(_) => {
            tracing::info!(
                "Vault unlocked successfully - CoreContext recreated with encrypted token store"
            );
            Ok(UnlockVaultResponse::ok("Vault unlocked successfully"))
        }
        Err(e) => {
            tracing::warn!("Vault unlock failed: {}", e);
            if matches!(e, BootError::InvalidPassword(_) | BootError::Decrypt(_)) {
                std::env::remove_var(VAULT_PASSWORD_ENV_VAR);
            }
            Ok(UnlockVaultResponse::failed(match e {
                BootError::Context(cause) => {
                    format!("Failed to initialize after unlock: {}", cause)
                }
                other => other.to_string(),
            }))
        }
    }
}

async fn unlock_keyring_session(
    app: &tauri::AppHandle, password: &str, authenticated: bool,
) -> Result<UnlockVaultResponse, ErrorResponse> {
    let core_loaded = app
        .state::<crate::MaybeCoreContext>()
        .0
        .read()
        .await
        .is_some();
    if core_loaded {
        if !password.is_empty() {
            std::env::set_var(VAULT_PASSWORD_ENV_VAR, password);
            tracing::info!("Vault password set for future migration (no encrypted tokens yet)");
        }
        return Ok(UnlockVaultResponse::ok("Vault password set for migration"));
    }

    match boot_core(app, BootKind::Resume, authenticated).await {
        Ok(_) => Ok(UnlockVaultResponse::ok("Vault unlocked successfully")),
        Err(e) => Ok(UnlockVaultResponse::failed(e.to_string())),
    }
}

#[tauri::command]
pub async fn lock_vault(app: tauri::AppHandle) -> Result<UnlockVaultResponse, ErrorResponse> {
    app_lock::lock_session(&app).await;

    Ok(UnlockVaultResponse::ok("Vault locked"))
}
