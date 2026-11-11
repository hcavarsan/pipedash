use pipedash_core::application::services::storage_service::{
    self,
    migration as storage_migration,
    TestConnectionResult,
};
use pipedash_core::infrastructure::{
    MigrationOptions,
    MigrationPlan,
    MigrationResult,
    PipedashConfig,
};
use tauri::{
    Manager,
    State,
};

use crate::app_lock;
#[cfg(desktop)]
use crate::core_boot::{
    boot_core,
    BootKind,
};
use crate::error::ErrorResponse;

#[tauri::command]
pub async fn test_storage_connection(
    maybe_core: State<'_, crate::MaybeCoreContext>, config: PipedashConfig,
) -> Result<TestConnectionResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;

    Ok(storage_service::test_connection(config, Some(core.token_store.clone()), true).await?)
}

#[tauri::command]
pub async fn execute_storage_migration(
    app: tauri::AppHandle, maybe_core: State<'_, crate::MaybeCoreContext>, plan: MigrationPlan,
    options: MigrationOptions,
) -> Result<MigrationResult, ErrorResponse> {
    let lock = app.state::<app_lock::AppLockState>();
    let _transition = lock.transition().await;
    #[cfg(desktop)]
    let dry_run = options.dry_run;

    let core = maybe_core.0.read().await.clone();
    let token_store = core.as_ref().map(|core| core.token_store.clone());
    let manager =
        storage_service::open_manager(plan.from.clone(), token_store.clone(), true).await?;
    #[cfg(desktop)]
    if !dry_run {
        crate::desktop::projection::detach(&app);
        crate::desktop::flush_menu(&app).await;
    }

    let result = storage_migration::execute_migration(
        &manager,
        plan,
        options,
        core.as_ref().map(|core| core.event_bus.clone()),
        token_store,
        true,
    )
    .await;
    #[cfg(desktop)]
    if !dry_run {
        drop(_transition);
        if boot_core(&app, BootKind::Resume, false).await.is_err() {
            crate::desktop::initialization_failed(
                &app,
                "Storage changed. Restart or unlock Pipedash to load the current configuration.",
            );
        }
    }
    Ok(result)
}
