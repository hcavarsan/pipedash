#[cfg(desktop)]
use pipedash_core::application::services::storage_service::vault::{
    requires_vault_password,
    vault_is_unlocked,
};
use serde::Serialize;
use tauri::State;

use crate::app_lock;
#[cfg(mobile)]
use crate::core_boot::{
    boot_core,
    BootKind,
};
use crate::error::ErrorResponse;

#[derive(Debug, Serialize)]
pub struct FactoryResetResult {
    pub providers_removed: usize,
    pub caches_cleared: bool,
    pub tokens_cleared: bool,
    pub metrics_cleared: bool,
}

#[tauri::command]
pub async fn factory_reset(
    app: tauri::AppHandle, maybe_core: State<'_, crate::MaybeCoreContext>,
    lock: State<'_, app_lock::AppLockState>,
) -> Result<FactoryResetResult, ErrorResponse> {
    tracing::info!("[factory_reset] Starting factory reset");

    let _transition = lock.transition().await;
    #[cfg(desktop)]
    let generation = crate::desktop::generation(&app);
    #[cfg(desktop)]
    {
        crate::desktop::projection::detach(&app);
        crate::desktop::flush_menu(&app).await;
    }
    #[cfg(mobile)]
    crate::build_watch::interrupt(&app);

    let core = maybe_core.get().await;
    let result = async {
        let core = core.as_ref().map_err(|e| ErrorResponse {
            error: e.clone(),
            details: None,
            code: None,
        })?;
        crate::local_notifications::settings::reset(&app).await?;

        let providers_removed = core.factory_reset().await?;

        app_lock::commands::reset(&app).await?;
        #[cfg(desktop)]
        crate::desktop::settings::reset_settings(&app).await?;

        tracing::info!(providers_removed, "[factory_reset] Complete");

        Ok(FactoryResetResult {
            providers_removed,
            caches_cleared: true,
            tokens_cleared: true,
            metrics_cleared: true,
        })
    }
    .await;

    #[cfg(desktop)]
    if let Ok(core) = core {
        let config = core.config_state.get().await;
        let vault_ready = vault_is_unlocked(
            requires_vault_password(&config).await,
            std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok(),
            lock.locked(),
        );
        crate::desktop::projection::attach(&app, core, generation, vault_ready).await;
    }
    result
}

#[tauri::command]
pub async fn restart_app(app: tauri::AppHandle) -> Result<(), ErrorResponse> {
    tracing::info!("[restart_app] Initiating application restart...");

    relaunch(&app).await
}

#[cfg(mobile)]
async fn relaunch(app: &tauri::AppHandle) -> Result<(), ErrorResponse> {
    boot_core(app, BootKind::Resume, false)
        .await
        .map(|_| ())
        .map_err(|e| ErrorResponse {
            error: e.to_string(),
            details: None,
            code: None,
        })
}

#[cfg(not(mobile))]
async fn relaunch(app: &tauri::AppHandle) -> Result<(), ErrorResponse> {
    app.restart()
}
