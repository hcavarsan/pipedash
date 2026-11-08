use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{
    Arc,
    PoisonError,
};

use pipedash_core::CoreContext;
use tauri::{
    AppHandle,
    Manager,
    Runtime,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_pipedash_native::{
    LocalNotificationSettings,
    PipedashNativeExt,
};

use super::events::observe;
use super::store::{
    self,
    AlertHost,
};
use super::{
    error,
    storage,
    LocalNotificationsState,
    Result,
};
use crate::app_lock::AppLockState;
use crate::error::ErrorResponse;
use crate::remote::alerts::revoke_device;
use crate::MaybeCoreContext;

pub(super) async fn initialize<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let state = app.state::<LocalNotificationsState>();
    state
        .initialized
        .get_or_try_init(|| async {
            let settings = app
                .pipedash_native()
                .local_notification_settings()
                .await
                .map_err(|_| error("Notification settings could not be read"))?;
            let enabled = if settings.enabled {
                let retained = match storage(store::authorization).await {
                    Ok(None) => storage(store::alert_host)
                        .await
                        .map_or(true, |host| host.is_some()),
                    Ok(Some(_)) | Err(_) => true,
                };
                if !retained {
                    app.pipedash_native()
                        .configure_local_notifications(false)
                        .await
                        .map_err(|_| error("Notification authorization must be renewed"))?;
                }
                retained
            } else {
                false
            };
            state.enabled.store(enabled, Ordering::Release);
            Ok::<(), ErrorResponse>(())
        })
        .await?;
    Ok(())
}

pub(super) async fn permission_granted<R: Runtime>(app: &AppHandle<R>) -> Result<bool> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || app.notification().permission_state())
        .await
        .map_err(|_| error("Notification permission could not be checked"))?
        .map(|permission| permission == tauri::plugin::PermissionState::Granted)
        .map_err(|_| error("Notification permission could not be checked"))
}

pub(super) async fn capture_authorization(core: &CoreContext) -> Result<store::Authorization> {
    let providers = core
        .provider_service
        .repository()
        .list_providers()
        .await
        .map_err(|_| error("Provider credentials could not be read for background monitoring"))?;
    if providers.iter().any(|provider| provider.token.is_empty()) {
        return Err(error(
            "Unlock the vault and check provider credentials before enabling notifications",
        ));
    }
    let config = core.config_state.get().await;
    Ok(store::Authorization {
        providers,
        proxy_url: config.general.proxy_url.clone(),
        status_pages_enabled: config.general.status_pages_enabled,
    })
}

pub(super) async fn unlocked_core<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<CoreContext>> {
    if app.state::<AppLockState>().locked() {
        return Err(error(
            "Unlock Pipedash before enabling background notifications",
        ));
    }
    app.state::<MaybeCoreContext>()
        .get()
        .await
        .map_err(|_| error("Unlock the vault before enabling background notifications"))
}

pub(crate) async fn current_alert_host() -> Option<AlertHost> {
    if !cfg!(mobile) {
        return None;
    }
    storage(store::alert_host).await.ok().flatten()
}

pub(crate) async fn mark_alert_host_push(host: &AlertHost, push: bool) {
    let host = host.clone();
    if cfg!(mobile)
        && storage(move || store::set_alert_host_push(&host, push).map(drop))
            .await
            .is_err()
    {
        tracing::debug!("Couldn't save the push state of the alert host");
    }
}

pub(crate) async fn notifications_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    initialize(app).await.is_ok()
        && app
            .state::<LocalNotificationsState>()
            .enabled
            .load(Ordering::Acquire)
}

pub(crate) async fn advance_alert_host(seq: i64) {
    if cfg!(mobile)
        && storage(move || store::advance_alert_host(seq))
            .await
            .is_err()
    {
        tracing::debug!("Couldn't save the alert position");
    }
}

pub(crate) async fn reset_alert_host_seq(seq: i64) {
    if cfg!(mobile)
        && storage(move || store::reset_alert_host_seq(seq))
            .await
            .is_err()
    {
        tracing::debug!("Couldn't save the alert position");
    }
}

pub(crate) fn set_push_problem<R: Runtime>(app: &AppHandle<R>, problem: Option<String>) {
    *app.state::<LocalNotificationsState>()
        .push_problem
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = problem;
}

pub(super) async fn detach_alert_host(app: &AppHandle, exiting: bool) {
    let Some(host) = current_alert_host().await else {
        return;
    };
    if host.standalone || !exiting {
        crate::push::disable(app).await;
    } else {
        crate::push::forget().await;
    }
    if host.standalone {
        revoke_device(&host).await;
    }
    if storage(store::remove_alert_host_record).await.is_err() {
        tracing::warn!("Couldn't remove the alert host");
    }
}

pub(super) async fn enable_push(app: &AppHandle) {
    if cfg!(target_os = "ios") {
        let problem = crate::push::enable(app).await.err();
        set_push_problem(app, problem);
    }
}

pub(super) async fn activate<R: Runtime>(app: &AppHandle<R>) -> Result<LocalNotificationSettings> {
    let state = app.state::<LocalNotificationsState>();
    match app
        .pipedash_native()
        .configure_local_notifications(true)
        .await
    {
        Ok(settings) if settings.enabled => {
            state.enabled.store(true, Ordering::Release);
            state.invalidate();
            Ok(settings)
        }
        _ => {
            storage(store::clear).await?;
            Err(error("The system could not enable notifications"))
        }
    }
}

pub(super) async fn disable<R: Runtime>(app: &AppHandle<R>) -> Result<LocalNotificationSettings> {
    let state = app.state::<LocalNotificationsState>();
    set_push_problem(app, None);
    state.enabled.store(false, Ordering::Release);
    state.invalidate();
    let configured = app
        .pipedash_native()
        .configure_local_notifications(false)
        .await;
    let _observations = state.observations.lock().await;
    let cleared = if cfg!(mobile) {
        storage(store::clear).await
    } else {
        Ok(())
    };
    let settings = configured
        .map_err(|_| error("The system could not cancel background notification checks"))?;
    cleared?;
    Ok(settings)
}

pub(crate) async fn reset(app: &AppHandle) -> Result<()> {
    if !cfg!(mobile) {
        return Ok(());
    }
    initialize(app).await?;
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    detach_alert_host(app, true).await;
    disable(app).await?;
    Ok(())
}

async fn sync_from_core<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    if !cfg!(mobile) {
        return Ok(());
    }
    initialize(app).await?;
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    if !state.enabled.load(Ordering::Acquire) {
        return Ok(());
    }
    if storage(store::authorization).await?.is_none() {
        if storage(store::alert_host).await?.is_none() {
            disable(app).await?;
        }
        return Ok(());
    }
    let Ok(core) = unlocked_core(app).await else {
        return Ok(());
    };
    state.invalidate();
    let authorization_revision = state.authorization_revision.load(Ordering::Acquire);
    let authorization = capture_authorization(&core).await?;
    let current = unlocked_core(app).await?;
    if !Arc::ptr_eq(&core, &current) {
        return Err(error(
            "The app session changed while updating background notifications",
        ));
    }
    let ids: HashSet<_> = authorization
        .providers
        .iter()
        .filter_map(|provider| provider.id)
        .collect();
    let guard = Arc::clone(&state.observations).lock_owned().await;
    if state.authorization_revision.load(Ordering::Acquire) != authorization_revision {
        return Ok(());
    }
    storage(move || store::save_authorization(&authorization)).await?;
    observe(app, |observations| {
        observations.retain_providers(&ids);
        Vec::new()
    })
    .await?;
    drop(guard);
    Ok(())
}

pub(crate) fn schedule_sync<R: Runtime>(app: &AppHandle<R>) {
    if !cfg!(mobile) {
        return;
    }
    if let Some(state) = app.try_state::<LocalNotificationsState>() {
        state.sync.notify_one();
    }
}

pub(super) async fn sync_worker<R: Runtime>(app: AppHandle<R>) {
    let state = app.state::<LocalNotificationsState>();
    loop {
        state.sync.notified().await;
        if sync_from_core(&app).await.is_err() {
            tracing::warn!("Could not synchronize authorized background notification credentials");
        }
    }
}
