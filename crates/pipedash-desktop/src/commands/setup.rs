use pipedash_core::application::services::setup_service::SetupService;
use pipedash_core::domain::setup::{
    SetupApplied,
    SetupDraft,
    SetupDraftResult,
};
use pipedash_core::infrastructure::{
    ConfigLoader,
    SetupStatus,
};
use tauri::{
    Manager,
    State,
};

use crate::core_boot::{
    boot_core,
    BootKind,
};
use crate::error::ErrorResponse;
use crate::{
    app_lock,
    AppDataDir,
    MaybeCoreContext,
};

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, ErrorResponse> + Send + 'static,
) -> Result<T, ErrorResponse> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Background task failed: {e}"),
            details: None,
            code: None,
        })?
}

#[tauri::command]
pub async fn check_setup_status(
    app_data_dir: State<'_, AppDataDir>,
) -> Result<SetupStatus, ErrorResponse> {
    let config_path = app_data_dir.0.join("config.toml");
    blocking(move || {
        Ok(ConfigLoader::get_setup_status(
            &config_path,
            &crate::plugin_registry::plugin_registry(),
        ))
    })
    .await
}

#[tauri::command]
pub async fn bootstrap_app(app: tauri::AppHandle) -> Result<(), ErrorResponse> {
    tracing::info!("Bootstrapping application after initial setup");

    boot_core(&app, BootKind::Setup, false)
        .await
        .map_err(|e| ErrorResponse {
            error: e.to_string(),
            details: None,
            code: None,
        })?;

    tracing::info!("Application bootstrap completed successfully");

    Ok(())
}

#[tauri::command]
pub async fn prepare_setup_draft(
    app_data_dir: State<'_, AppDataDir>, draft: SetupDraft,
) -> Result<SetupDraftResult, ErrorResponse> {
    let service = SetupService::new(app_data_dir.0.join("config.toml"), None, true);

    Ok(service.prepare(&draft).await?)
}

#[tauri::command]
pub async fn apply_setup_draft(
    app: tauri::AppHandle, maybe_core: State<'_, MaybeCoreContext>,
    app_data_dir: State<'_, AppDataDir>, draft: SetupDraft,
) -> Result<SetupApplied, ErrorResponse> {
    let lock = app.state::<app_lock::AppLockState>();
    let transition = lock.transition().await;
    let core = maybe_core.0.read().await.clone();
    let reconfiguring = core.is_some();
    let service = SetupService::new(app_data_dir.0.join("config.toml"), core, true);

    #[cfg(desktop)]
    if reconfiguring {
        crate::desktop::projection::detach(&app);
        crate::desktop::flush_menu(&app).await;
    }

    let applied = service.apply(draft).await;

    #[cfg(desktop)]
    if reconfiguring {
        drop(transition);
        if boot_core(&app, BootKind::Resume, false).await.is_err() {
            crate::desktop::initialization_failed(
                &app,
                "Storage changed. Restart or unlock Pipedash to load the current configuration.",
            );
        }
    }
    #[cfg(not(desktop))]
    let _ = (transition, reconfiguring);

    Ok(applied?)
}
