mod store;

use std::collections::HashSet;
use std::sync::{
    atomic::{
        AtomicBool,
        AtomicU64,
        Ordering,
    },
    Arc,
    Mutex,
    PoisonError,
};
use std::time::Duration;

use pipedash_core::application::{
    refresh_notifications,
    LocalNotification,
    NotificationState,
};
use pipedash_core::event::CoreEvent;
use pipedash_core::CoreContext;
use tauri::{
    AppHandle,
    Manager,
    Runtime,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_pipedash_native::{
    LocalNotificationSettings,
    NotificationRefresh,
    PipedashNativeExt,
};
use tokio::sync::{
    watch,
    Mutex as AsyncMutex,
    OnceCell,
};

use crate::app_lock::AppLockState;
use crate::commands::ErrorResponse;
use crate::native::NotificationTarget;
use crate::MaybeCoreContext;

const REFRESH_BUDGET: Duration = Duration::from_secs(23);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(26);

type Result<T, E = ErrorResponse> = std::result::Result<T, E>;

#[derive(Default)]
pub struct LocalNotificationsState {
    initialized: OnceCell<()>,
    enabled: AtomicBool,
    revision: AtomicU64,
    authorization_revision: AtomicU64,
    settings: AsyncMutex<()>,
    observations: Arc<AsyncMutex<()>>,
    refresh: Mutex<Option<ActiveRefresh>>,
}

struct ActiveRefresh {
    id: String,
    cancel: watch::Sender<bool>,
}

impl LocalNotificationsState {
    fn start_refresh(&self, id: &str) -> Option<watch::Receiver<bool>> {
        let mut active = self.refresh.lock().unwrap_or_else(PoisonError::into_inner);
        if active.is_some() || !self.enabled.load(Ordering::Acquire) {
            return None;
        }
        let (cancel, cancelled) = watch::channel(false);
        *active = Some(ActiveRefresh {
            id: id.to_owned(),
            cancel,
        });
        Some(cancelled)
    }

    fn cancel_refresh(&self, id: Option<&str>) {
        let mut active = self.refresh.lock().unwrap_or_else(PoisonError::into_inner);
        if id.is_some_and(|id| active.as_ref().is_none_or(|task| task.id != id)) {
            return;
        }
        if let Some(task) = active.take() {
            let _ = task.cancel.send(true);
        }
    }

    fn invalidate(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.authorization_revision.fetch_add(1, Ordering::AcqRel);
        self.cancel_refresh(None);
    }
}

fn error(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_owned(),
        details: Some("LOCAL_NOTIFICATIONS".to_owned()),
    }
}

async fn storage<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| error("Background notification storage could not be accessed"))?
}

async fn initialize<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
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
                match storage(store::authorization).await {
                    Ok(None) => {
                        app.pipedash_native()
                            .configure_local_notifications(false)
                            .await
                            .map_err(|_| error("Notification authorization must be renewed"))?;
                        false
                    }
                    Ok(Some(_)) | Err(_) => true,
                }
            } else {
                false
            };
            state.enabled.store(enabled, Ordering::Release);
            Ok::<(), ErrorResponse>(())
        })
        .await?;
    Ok(())
}

async fn permission_granted<R: Runtime>(app: &AppHandle<R>) -> Result<bool> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || app.notification().permission_state())
        .await
        .map_err(|_| error("Notification permission could not be checked"))?
        .map(|permission| permission == tauri::plugin::PermissionState::Granted)
        .map_err(|_| error("Notification permission could not be checked"))
}

async fn capture_authorization(core: &CoreContext) -> Result<store::Authorization> {
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

async fn unlocked_core<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<CoreContext>> {
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

#[tauri::command]
pub async fn get_local_notification_settings(app: AppHandle) -> Result<LocalNotificationSettings> {
    initialize(&app).await?;
    app.pipedash_native()
        .local_notification_settings()
        .await
        .map_err(|_| error("Notification settings could not be read"))
}

#[tauri::command]
pub async fn set_local_notifications_enabled(
    app: AppHandle, enabled: bool,
) -> Result<LocalNotificationSettings> {
    initialize(&app).await?;
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    if !enabled {
        return disable(&app).await;
    }
    if !cfg!(target_os = "ios") {
        return Err(error("Background notifications require iOS"));
    }
    if !permission_granted(&app).await? {
        return Err(error("Allow notifications for Pipedash in iOS Settings"));
    }
    if state.enabled.load(Ordering::Acquire) {
        return app
            .pipedash_native()
            .local_notification_settings()
            .await
            .map_err(|_| error("Notification settings could not be read"));
    }
    let core = unlocked_core(&app).await?;
    let authorization_revision = state.authorization_revision.load(Ordering::Acquire);
    let authorization = capture_authorization(&core).await?;
    let pipelines = core.pipeline_service.get_cached_pipelines(None).await?;
    let providers = core.provider_service.list_providers().await?;
    let mut observations = NotificationState::default();
    observations.seed(&pipelines, &providers);
    let current = unlocked_core(&app).await?;
    if !Arc::ptr_eq(&core, &current) {
        return Err(error(
            "The app session changed while enabling notifications. Try again.",
        ));
    }
    let guard = Arc::clone(&state.observations).lock_owned().await;
    if state.authorization_revision.load(Ordering::Acquire) != authorization_revision {
        return Err(error(
            "Providers changed while enabling notifications. Try again.",
        ));
    }
    storage(move || {
        let _guard = guard;
        store::save_authorization(&authorization)?;
        store::save_observations(&observations)
    })
    .await?;
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
            Err(error("iOS could not enable notifications"))
        }
    }
}

async fn disable<R: Runtime>(app: &AppHandle<R>) -> Result<LocalNotificationSettings> {
    let state = app.state::<LocalNotificationsState>();
    state.enabled.store(false, Ordering::Release);
    state.invalidate();
    let configured = app
        .pipedash_native()
        .configure_local_notifications(false)
        .await;
    let _observations = state.observations.lock().await;
    let cleared = if cfg!(target_os = "ios") {
        storage(store::clear).await
    } else {
        Ok(())
    };
    let settings =
        configured.map_err(|_| error("iOS could not cancel background notification checks"))?;
    cleared?;
    Ok(settings)
}

pub async fn reset<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    if !cfg!(target_os = "ios") {
        return Ok(());
    }
    initialize(app).await?;
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    disable(app).await?;
    Ok(())
}

pub async fn sync_from_core<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    if !cfg!(target_os = "ios") {
        return Ok(());
    }
    initialize(app).await?;
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    if !state.enabled.load(Ordering::Acquire) {
        return Ok(());
    }
    if storage(store::authorization).await?.is_none() {
        disable(app).await?;
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
    storage(move || {
        let _guard = guard;
        let mut observations = store::observations()?;
        observations.retain_providers(&ids);
        store::save_authorization(&authorization)?;
        store::save_observations(&observations)
    })
    .await
}

pub fn schedule_sync<R: Runtime>(app: &AppHandle<R>) {
    if !cfg!(target_os = "ios") {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if sync_from_core(&app).await.is_err() {
            tracing::warn!("Could not synchronize authorized background notification credentials");
        }
    });
}

async fn deliver<R: Runtime>(app: &AppHandle<R>, notices: Vec<LocalNotification>) -> Result<()> {
    if notices.is_empty() || !permission_granted(app).await? {
        return Ok(());
    }
    for notice in notices {
        if !app
            .state::<LocalNotificationsState>()
            .enabled
            .load(Ordering::Acquire)
        {
            break;
        }
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::system_notifications::show_system_notification(
                app,
                notice.title,
                notice.body,
                Some(NotificationTarget {
                    key: Some(notice.key),
                    pipeline_id: notice.pipeline_id,
                    provider_id: notice.provider_id.and_then(|id| u64::try_from(id).ok()),
                    run_number: None,
                }),
            )
        })
        .await
        .map_err(|_| error("A local notification could not be delivered"))??;
    }
    Ok(())
}

pub async fn on_event<R: Runtime>(app: &AppHandle<R>, event: &CoreEvent) {
    if !cfg!(target_os = "ios") {
        return;
    }
    if matches!(
        event,
        CoreEvent::ProviderAdded { .. }
            | CoreEvent::ProviderUpdated { .. }
            | CoreEvent::ProvidersChanged
            | CoreEvent::VaultUnlocked
            | CoreEvent::MigrationComplete { success: true, .. }
    ) {
        if let Some(state) = app.try_state::<LocalNotificationsState>() {
            state.invalidate();
        }
        schedule_sync(app);
    }
    if !matches!(
        event,
        CoreEvent::PipelinesUpdated { .. }
            | CoreEvent::PipelinesFailed { .. }
            | CoreEvent::ProviderStatusUpdated { .. }
            | CoreEvent::ProviderHealthAlert { .. }
            | CoreEvent::ProviderRemoved { .. }
    ) {
        return;
    }
    if process_event(app, event).await.is_err() {
        tracing::warn!("Could not process a local notification observation");
    }
}

async fn process_event<R: Runtime>(app: &AppHandle<R>, event: &CoreEvent) -> Result<()> {
    initialize(app).await?;
    let state = app.state::<LocalNotificationsState>();
    if !state.enabled.load(Ordering::Acquire) {
        return Ok(());
    }
    let guard = Arc::clone(&state.observations).lock_owned().await;
    if !state.enabled.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut observations = storage(store::observations).await?;
    let authorization = if let CoreEvent::ProviderRemoved { provider, .. } = event {
        state.invalidate();
        let mut authorization = storage(store::authorization).await?;
        if let Some(authorization) = &mut authorization {
            authorization
                .providers
                .retain(|entry| entry.id != Some(provider.id));
            let ids = authorization
                .providers
                .iter()
                .filter_map(|entry| entry.id)
                .collect();
            observations.retain_providers(&ids);
        }
        authorization
    } else {
        None
    };
    let notices = observations.observe_event(event);
    deliver(app, notices).await?;
    storage(move || {
        let _guard = guard;
        if let Some(authorization) = authorization {
            store::save_authorization(&authorization)?;
        }
        store::save_observations(&observations)
    })
    .await?;
    state.revision.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

async fn refresh<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let state = app.state::<LocalNotificationsState>();
    let (authorization, mut observations, revision) = {
        let _guard = state.observations.lock().await;
        let authorization = storage(store::authorization)
            .await?
            .ok_or_else(|| error("Enable notifications again to authorize background checks"))?;
        let observations = storage(store::observations).await?;
        (
            authorization,
            observations,
            state.revision.load(Ordering::Acquire),
        )
    };
    let notices = refresh_notifications(
        &authorization.providers,
        &authorization.proxy_url,
        authorization.status_pages_enabled,
        &mut observations,
        REFRESH_BUDGET,
    )
    .await
    .map_err(|_| error("Background notification refresh could not finish"))?;
    let guard = Arc::clone(&state.observations).lock_owned().await;
    if !state.enabled.load(Ordering::Acquire) || state.revision.load(Ordering::Acquire) != revision
    {
        return Ok(());
    }
    deliver(app, notices).await?;
    storage(move || {
        let _guard = guard;
        store::save_observations(&observations)
    })
    .await?;
    state.revision.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

async fn run_refresh<R: Runtime>(
    app: AppHandle<R>, id: String, cancelled: Option<watch::Receiver<bool>>,
) {
    let success = if let Some(mut cancelled) = cancelled {
        tokio::select! {
            biased;
            _ = cancelled.changed() => false,
            result = tokio::time::timeout(REFRESH_TIMEOUT, refresh(&app)) => matches!(result, Ok(Ok(()))),
        }
    } else {
        false
    };
    app.state::<LocalNotificationsState>()
        .cancel_refresh(Some(&id));
    if app
        .pipedash_native()
        .finish_notification_refresh(&id, success)
        .await
        .is_err()
    {
        tracing::warn!("Could not complete the native notification refresh task");
    }
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if !cfg!(target_os = "ios") {
        return Ok(());
    }
    let handle = app.clone();
    app.pipedash_native()
        .on_notification_refresh(move |event: NotificationRefresh| {
            if event.expired {
                handle
                    .state::<LocalNotificationsState>()
                    .cancel_refresh(Some(&event.id));
            } else {
                let cancelled = handle
                    .state::<LocalNotificationsState>()
                    .start_refresh(&event.id);
                let handle = handle.clone();
                tauri::async_runtime::spawn(run_refresh(handle, event.id, cancelled));
            }
        })
        .map_err(|failure| tauri::Error::Anyhow(failure.into()))?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if initialize(&app).await.is_err() {
            tracing::warn!("Could not load local notification settings");
        }
        if app
            .pipedash_native()
            .notification_refresh_ready()
            .await
            .is_err()
        {
            tracing::warn!("Could not ready the native notification refresh handler");
        }
        schedule_sync(&app);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_notifications_cannot_start_refresh_and_old_expiration_cannot_cancel_new_task() {
        let state = LocalNotificationsState::default();
        assert!(state.start_refresh("old").is_none());
        state.enabled.store(true, Ordering::Release);
        let old = state.start_refresh("old").unwrap();
        assert!(state.start_refresh("duplicate").is_none());
        state.cancel_refresh(Some("old"));
        assert!(*old.borrow());
        let current = state.start_refresh("new").unwrap();
        state.cancel_refresh(Some("old"));
        assert!(!*current.borrow());
        state.enabled.store(false, Ordering::Release);
        state.invalidate();
        assert!(*current.borrow());
        assert!(state.start_refresh("disabled").is_none());
    }
}
