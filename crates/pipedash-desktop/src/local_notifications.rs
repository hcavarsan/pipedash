#[cfg(any(target_os = "android", test))]
mod file_vault;
#[cfg(target_os = "android")]
mod headless;
mod store;
mod wake;

use std::collections::HashSet;
use std::path::Path;
use std::sync::{
    atomic::{
        AtomicBool,
        AtomicU64,
        Ordering,
    },
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use pipedash_core::application::{
    refresh_notifications,
    LocalNotification,
    NotificationState,
    ObservedNotification,
    ShownNotification,
};
use pipedash_core::domain::{
    NotificationPreferences,
    OnceWatch,
    RunFinish,
};
#[cfg(mobile)]
use pipedash_core::event::CoreEvent;
use pipedash_core::CoreContext;
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
pub(crate) use store::AlertHost;
use tauri::{
    AppHandle,
    Manager,
    Runtime,
    State,
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
use crate::remote::alerts::{
    claim_alert_host,
    client_alert_host,
    client_request,
    host_request,
    revoke_device,
};
use crate::remote::{
    parse_pairing,
    RemoteConnection,
    RemoteError,
};
use crate::{
    AppDataDir,
    MaybeCoreContext,
};

const REFRESH_BUDGET: Duration = Duration::from_secs(23);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(26);

type Result<T, E = ErrorResponse> = std::result::Result<T, E>;

static PREFERENCES_SYNC: AsyncMutex<()> = AsyncMutex::const_new(());
static HOST_PREFERENCES_PENDING: AtomicBool = AtomicBool::new(false);
const PREFERENCES_RETRY_BUDGET: Duration = Duration::from_secs(3);

#[cfg(target_os = "android")]
pub(crate) use headless::{
    cancel as cancel_headless,
    run as refresh_headless,
};

#[cfg(target_os = "android")]
pub fn set_storage_dir(app_data_dir: &std::path::Path) {
    store::set_dir(app_data_dir.join("background-notifications"));
}

#[derive(Default)]
pub struct LocalNotificationsState {
    initialized: OnceCell<()>,
    enabled: AtomicBool,
    revision: AtomicU64,
    authorization_revision: AtomicU64,
    settings: AsyncMutex<()>,
    observations: Arc<AsyncMutex<()>>,
    refresh: Mutex<Option<ActiveRefresh>>,
    push_problem: Mutex<Option<String>>,
    preferences: Mutex<NotificationPreferences>,
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

    fn preferences(&self) -> MutexGuard<'_, NotificationPreferences> {
        self.preferences
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    #[cfg(desktop)]
    pub(crate) fn preferences_copy(&self) -> NotificationPreferences {
        self.preferences().clone()
    }

    pub(crate) fn admit(&self, notification: &ObservedNotification) -> Option<ShownNotification> {
        notification.admit(&mut self.preferences())
    }

    pub(crate) fn merge_preferences(
        &self, incoming: &NotificationPreferences,
    ) -> (NotificationPreferences, bool) {
        let mut current = self.preferences();
        let changed = store::merge(&mut current, incoming);

        (current.clone(), changed)
    }
}

fn error(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_owned(),
        details: Some("LOCAL_NOTIFICATIONS".to_owned()),
        code: None,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertHostInfo {
    name: String,
    push: bool,
    standalone: bool,
}

impl From<&AlertHost> for AlertHostInfo {
    fn from(host: &AlertHost) -> Self {
        Self {
            name: host.host_name.clone(),
            push: host.push,
            standalone: host.standalone,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    enabled: bool,
    available: bool,
    reason: Option<String>,
    alert_host: Option<AlertHostInfo>,
    push_problem: Option<String>,
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

async fn settings_view<R: Runtime>(
    app: &AppHandle<R>, native: LocalNotificationSettings,
) -> NotificationSettings {
    let host = current_alert_host().await;
    let push_problem = app
        .state::<LocalNotificationsState>()
        .push_problem
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();

    NotificationSettings {
        enabled: native.enabled,
        available: native.available,
        reason: native.reason,
        alert_host: host.as_ref().map(AlertHostInfo::from),
        push_problem,
    }
}

async fn native_view<R: Runtime>(app: &AppHandle<R>) -> Result<NotificationSettings> {
    let native = app
        .pipedash_native()
        .local_notification_settings()
        .await
        .map_err(|_| error("Notification settings could not be read"))?;

    Ok(settings_view(app, native).await)
}

async fn detach_alert_host(app: &AppHandle, exiting: bool) {
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

async fn enable_push(app: &AppHandle) {
    if cfg!(target_os = "ios") {
        let problem = crate::push::enable(app).await.err();
        set_push_problem(app, problem);
    }
}

async fn activate<R: Runtime>(app: &AppHandle<R>) -> Result<LocalNotificationSettings> {
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

#[tauri::command]
pub async fn get_local_notification_settings(app: AppHandle) -> Result<NotificationSettings> {
    initialize(&app).await?;
    native_view(&app).await
}

#[tauri::command]
pub async fn set_local_notifications_enabled(
    app: AppHandle, remote: State<'_, RemoteConnection>, enabled: bool,
) -> Result<NotificationSettings> {
    initialize(&app).await?;
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    if !enabled {
        detach_alert_host(&app, false).await;
        let native = disable(&app).await?;
        return Ok(settings_view(&app, native).await);
    }
    if !cfg!(mobile) {
        return Err(error(
            "Background notifications require the Pipedash mobile app",
        ));
    }
    if !permission_granted(&app).await? {
        return Err(error("Allow notifications for Pipedash in Settings"));
    }
    if state.enabled.load(Ordering::Acquire) {
        return native_view(&app).await;
    }
    set_push_problem(&app, None);
    if remote.is_client() {
        let host = client_alert_host(&remote).await?;
        let saved = host.clone();
        storage(move || store::save_alert_host(&saved)).await?;
        let native = activate(&app).await?;
        sync_host_preferences(&host).await;
        enable_push(&app).await;
        return Ok(settings_view(&app, native).await);
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
    let native = activate(&app).await?;

    Ok(settings_view(&app, native).await)
}

#[tauri::command]
pub async fn pair_alert_host(
    app: AppHandle, remote: State<'_, RemoteConnection>, address: String, code: Option<String>,
    allow_private_http: bool,
) -> Result<AlertHostInfo> {
    if remote.is_client() {
        return Err(RemoteError::Connected.into());
    }
    if !cfg!(mobile) {
        return Err(error("Alerts from a server need the Pipedash mobile app"));
    }
    initialize(&app).await?;
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    if !state.enabled.load(Ordering::Acquire) {
        return Err(error("Turn on notifications first."));
    }

    let (url, code) = parse_pairing(&address, code.as_deref())?;
    let host = claim_alert_host(url, &code, allow_private_http).await?;
    detach_alert_host(&app, false).await;
    let saved = host.clone();
    if let Err(failure) = storage(move || store::save_alert_host(&saved)).await {
        revoke_device(&host).await;
        return Err(failure);
    }
    sync_host_preferences(&host).await;
    set_push_problem(&app, None);
    enable_push(&app).await;

    Ok(AlertHostInfo::from(
        &current_alert_host().await.unwrap_or(host),
    ))
}

#[tauri::command]
pub async fn remove_alert_host(app: AppHandle) -> Result<()> {
    let state = app.state::<LocalNotificationsState>();
    let _settings = state.settings.lock().await;
    detach_alert_host(&app, false).await;
    set_push_problem(&app, None);

    Ok(())
}

async fn disable<R: Runtime>(app: &AppHandle<R>) -> Result<LocalNotificationSettings> {
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

pub async fn reset(app: &AppHandle) -> Result<()> {
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

pub async fn sync_from_core<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
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

pub fn schedule_sync<R: Runtime>(app: &AppHandle<R>) {
    if !cfg!(mobile) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if sync_from_core(&app).await.is_err() {
            tracing::warn!("Could not synchronize authorized background notification credentials");
        }
    });
}

fn wanted(
    preferences: &NotificationPreferences, notices: Vec<LocalNotification>,
) -> Vec<LocalNotification> {
    notices
        .into_iter()
        .filter(|notice| {
            preferences.allows(
                notice.kind,
                &notice.provider_ids,
                notice.pipeline_id.as_deref(),
                notice.finish.as_ref(),
            )
        })
        .collect()
}

async fn remember_finishes(finishes: Vec<(String, RunFinish)>) {
    if finishes.is_empty() {
        return;
    }
    let _sync = PREFERENCES_SYNC.lock().await;
    if storage(move || store::fire_once(&finishes)).await.is_err() {
        tracing::warn!("Couldn't mark a one-time watch as used");
    }
}

const PREFERENCES_PATH: &str = "/devices/current/notification-preferences";

#[derive(Deserialize)]
struct SavedPreferences {
    #[serde(default)]
    fired: Vec<OnceWatch>,
}

#[derive(Deserialize)]
struct ServerPreferences {
    preferences: Option<NotificationPreferences>,
}

enum Server<'a> {
    Host(&'a AlertHost),
    Client(&'a RemoteConnection, &'a Path),
}

impl Server<'_> {
    async fn request(
        &self, method: &str, path: &str, body: Option<&str>,
    ) -> Result<(u16, String), RemoteError> {
        match self {
            Self::Host(host) => host_request(host, method, path, body).await,
            Self::Client(remote, _) => client_request(remote, method, path, body).await,
        }
    }

    fn set_pending(&self, pending: bool) {
        match self {
            Self::Host(_) => HOST_PREFERENCES_PENDING.store(pending, Ordering::Release),
            Self::Client(remote, dir) => remote.set_preferences_pending(dir, pending),
        }
    }
}

fn parse_answer<T: DeserializeOwned>(status: u16, body: &str) -> Option<T> {
    (status == 200)
        .then(|| serde_json::from_str(body).ok())
        .flatten()
}

async fn send_preferences(
    server: &Server<'_>, preferences: &NotificationPreferences,
) -> Vec<OnceWatch> {
    let body = match server {
        Server::Host(host) if host.standalone => serde_json::to_string(&preferences.kinds_only()),
        _ => serde_json::to_string(preferences),
    };
    let saved = match body {
        Ok(body) => server
            .request("PUT", PREFERENCES_PATH, Some(&body))
            .await
            .ok()
            .and_then(|(status, body)| parse_answer::<SavedPreferences>(status, &body)),
        Err(_) => None,
    };
    if saved.is_none() {
        tracing::debug!("Couldn't send the notification preferences to the alert host");
    }
    server.set_pending(saved.is_none());

    saved.map(|saved| saved.fired).unwrap_or_default()
}

async fn sync_host_preferences(host: &AlertHost) {
    let _sync = PREFERENCES_SYNC.lock().await;
    if let Ok(preferences) = storage(store::preferences).await {
        send_preferences(&Server::Host(host), &preferences).await;
    }
}

async fn retry_host_preferences(host: &AlertHost) {
    if HOST_PREFERENCES_PENDING.load(Ordering::Acquire)
        && tokio::time::timeout(PREFERENCES_RETRY_BUDGET, sync_host_preferences(host))
            .await
            .is_err()
    {
        tracing::debug!("The alert host took too long to take the notification preferences");
    }
}

fn used_watches(mut known: Vec<OnceWatch>, from_server: Vec<OnceWatch>) -> Vec<OnceWatch> {
    known.extend(from_server);
    known.sort_by(|a, b| (&a.pipeline_id, a.since).cmp(&(&b.pipeline_id, b.since)));
    known.dedup_by(|a, b| a.pipeline_id == b.pipeline_id && a.since == b.since);

    known
}

fn preferences_changed(app: &AppHandle) {
    #[cfg(desktop)]
    crate::desktop::tray_changed(app);
    #[cfg(not(desktop))]
    let _ = app;
}

async fn merge_local(
    app: &AppHandle, state: &LocalNotificationsState, incoming: NotificationPreferences,
) -> Result<(NotificationPreferences, bool)> {
    let (incoming, stored_changed) = if cfg!(mobile) {
        storage(move || store::merge_preferences(&incoming)).await?
    } else {
        (incoming, false)
    };
    let (merged, changed) = state.merge_preferences(&incoming);
    if changed {
        preferences_changed(app);
    }

    Ok((merged, changed || stored_changed))
}

#[tauri::command]
pub async fn set_notification_preferences(
    app: AppHandle, state: State<'_, LocalNotificationsState>, remote: State<'_, RemoteConnection>,
    data_dir: State<'_, AppDataDir>, preferences: NotificationPreferences,
) -> Result<Vec<OnceWatch>> {
    let _sync = PREFERENCES_SYNC.lock().await;
    let (preferences, changed) = merge_local(&app, &state, preferences).await?;
    let mut sent = Vec::new();
    let client = remote.is_client();
    let pending = if client {
        remote.preferences_pending()
    } else {
        HOST_PREFERENCES_PENDING.load(Ordering::Acquire)
    };
    if changed || pending {
        let host = if client {
            None
        } else {
            current_alert_host().await
        };
        let server = if client {
            Some(Server::Client(&remote, &data_dir.0))
        } else {
            host.as_ref().map(Server::Host)
        };
        match server {
            Some(server) => sent = send_preferences(&server, &preferences).await,
            None => HOST_PREFERENCES_PENDING.store(false, Ordering::Release),
        }
    }

    Ok(used_watches(preferences.fired_watches(), sent))
}

#[tauri::command]
pub async fn load_notification_preferences(
    app: AppHandle, state: State<'_, LocalNotificationsState>, remote: State<'_, RemoteConnection>,
) -> Result<Option<NotificationPreferences>> {
    if !remote.is_client() {
        return Ok(None);
    }
    let _sync = PREFERENCES_SYNC.lock().await;
    if remote.preferences_pending() {
        return Ok(None);
    }
    let (status, body) = client_request(&remote, "GET", PREFERENCES_PATH, None).await?;
    let saved = parse_answer::<ServerPreferences>(status, &body)
        .ok_or_else(|| error("The host sent notification preferences Pipedash can't read."))?;
    let Some(preferences) = saved.preferences else {
        return Ok(None);
    };

    Ok(Some(merge_local(&app, &state, preferences).await?.0))
}

async fn deliver<R: Runtime>(
    app: &AppHandle<R>, preferences: &NotificationPreferences, notices: Vec<LocalNotification>,
) -> Result<Vec<(String, RunFinish)>> {
    let notices = wanted(preferences, notices);
    if notices.is_empty() || !permission_granted(app).await? {
        return Ok(Vec::new());
    }
    let mut shown = Vec::new();
    for notice in notices {
        if !app
            .state::<LocalNotificationsState>()
            .enabled
            .load(Ordering::Acquire)
        {
            break;
        }
        let finished = notice.pipeline_id.clone().zip(notice.finish);
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
        shown.extend(finished);
    }
    Ok(shown)
}

#[cfg(mobile)]
pub async fn on_event<R: Runtime>(app: &AppHandle<R>, event: &CoreEvent) {
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

#[cfg(mobile)]
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
    let authorization = if let CoreEvent::ProviderRemoved { provider, .. } = event {
        state.invalidate();
        let mut authorization = storage(store::authorization).await?;
        if let Some(authorization) = &mut authorization {
            authorization
                .providers
                .retain(|entry| entry.id != Some(provider.id));
        }
        authorization
    } else {
        None
    };
    let kept: Option<HashSet<i64>> = authorization.as_ref().map(|authorization| {
        authorization
            .providers
            .iter()
            .filter_map(|entry| entry.id)
            .collect()
    });
    if let Some(authorization) = authorization {
        storage(move || store::save_authorization(&authorization)).await?;
    }
    observe(app, |observations| {
        if let Some(ids) = &kept {
            observations.retain_providers(ids);
        }
        observations.observe_event(event)
    })
    .await?;
    drop(guard);
    state.revision.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

#[cfg(all(target_os = "android", not(test)))]
async fn observe<R: Runtime>(
    app: &AppHandle<R>, change: impl FnOnce(&mut NotificationState) -> Vec<LocalNotification>,
) -> Result<()> {
    let (lock, mut observations, preferences) = storage(store::lock_observations).await?;
    let notices = change(&mut observations);
    let shown = deliver(app, &preferences, notices).await?;
    storage(move || lock.save(&observations)).await?;
    tauri::async_runtime::spawn(remember_finishes(shown));
    Ok(())
}

#[cfg(not(all(target_os = "android", not(test))))]
async fn observe<R: Runtime>(
    app: &AppHandle<R>, change: impl FnOnce(&mut NotificationState) -> Vec<LocalNotification>,
) -> Result<()> {
    let (mut observations, preferences) =
        storage(|| Ok((store::observations()?, store::preferences()?))).await?;
    let notices = change(&mut observations);
    let shown = deliver(app, &preferences, notices).await?;
    storage(move || store::save_observations(&observations)).await?;
    tauri::async_runtime::spawn(remember_finishes(shown));
    Ok(())
}

async fn refresh<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let state = app.state::<LocalNotificationsState>();
    let (authorization, mut observations, host, revision) = {
        let _guard = state.observations.lock().await;
        let authorization = storage(store::authorization).await?;
        let saved_host = storage(store::alert_host).await?;
        if authorization.is_none() && saved_host.is_none() {
            return Err(error(
                "Enable notifications again to authorize background checks",
            ));
        }
        let host = saved_host;
        let observations = if authorization.is_some() {
            storage(store::observations).await?
        } else {
            NotificationState::default()
        };
        (
            authorization,
            observations,
            host,
            state.revision.load(Ordering::Acquire),
        )
    };
    let mut wake = wake::gather(
        async {
            match &authorization {
                Some(authorization) => refresh_notifications(
                    &authorization.providers,
                    &authorization.proxy_url,
                    authorization.status_pages_enabled,
                    &mut observations,
                    REFRESH_BUDGET,
                )
                .await
                .ok(),
                None => None,
            }
        },
        async {
            match &host {
                Some(host) => wake::fetch_host(host, REFRESH_BUDGET).await,
                None => None,
            }
        },
    )
    .await;
    if wake.scanned.is_none() && wake.hosted.is_none() {
        return Err(error("Background notification refresh could not finish"));
    }
    let guard = Arc::clone(&state.observations).lock_owned().await;
    if !state.enabled.load(Ordering::Acquire) || state.revision.load(Ordering::Acquire) != revision
    {
        return Ok(());
    }
    if let Some(fetched) = host.clone() {
        if !storage(move || store::is_alert_host(&fetched))
            .await
            .unwrap_or(false)
        {
            wake.hosted = None;
        }
    }
    let notices = wake.take_notices();
    let preferences = storage(store::preferences).await?;
    let used = deliver(app, &preferences, notices).await?;
    let scanned = wake.scanned.is_some();
    let cursor = wake.hosted;
    let shown = host
        .clone()
        .zip(cursor.as_ref().and_then(|batch| batch.ack));
    let retry = host.clone();
    storage(move || {
        let _guard = guard;
        if scanned {
            store::save_observations(&observations)?;
        }
        match (&host, &cursor) {
            (Some(host), Some(batch)) => wake::save_cursor(host, batch).map(drop),
            _ => Ok(()),
        }
    })
    .await?;
    tauri::async_runtime::spawn(remember_finishes(used));
    if let Some((host, through)) = shown {
        wake::acknowledge(&host, through).await;
    }
    if let Some(host) = &retry {
        retry_host_preferences(host).await;
    }
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
    if !cfg!(mobile) {
        return Ok(());
    }
    HOST_PREFERENCES_PENDING.store(true, Ordering::Release);
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
        {
            let _sync = PREFERENCES_SYNC.lock().await;
            if let Ok(stored) = storage(store::preferences).await {
                app.state::<LocalNotificationsState>()
                    .merge_preferences(&stored);
            }
        }
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
    use chrono::{
        Duration as Span,
        Utc,
    };
    use pipedash_core::domain::AlertKind;

    use super::*;

    fn observed(
        id: u64, kind: AlertKind, pipeline_id: &str, finish: Option<RunFinish>,
    ) -> ObservedNotification {
        ObservedNotification {
            id,
            at: Utc::now(),
            notice: LocalNotification {
                key: format!("key:{id}"),
                title: "title".to_owned(),
                body: "body".to_owned(),
                pipeline_id: Some(pipeline_id.to_owned()),
                provider_id: Some(1),
                provider_ids: vec![1],
                kind,
                finish,
                level: None,
                link: None,
            },
        }
    }

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

    #[test]
    fn the_app_copy_decides_what_is_shown_and_uses_a_one_time_watch_once() {
        let state = LocalNotificationsState::default();
        let request = OnceWatch {
            pipeline_id: "github__1__api".to_owned(),
            since: Utc::now(),
            fired_at: None,
        };
        let chosen = NotificationPreferences {
            failed: false,
            watch_once: vec![request.clone()],
            ..NotificationPreferences::default()
        };
        assert!(state.merge_preferences(&chosen).1);

        assert!(state
            .admit(&observed(1, AlertKind::Failed, "github__1__web", None))
            .is_none());

        let finish = RunFinish {
            at: request.since + Span::minutes(1),
            previous: None,
        };
        let shown = state
            .admit(&observed(
                2,
                AlertKind::Finished,
                "github__1__api",
                Some(finish),
            ))
            .unwrap();
        let used = OnceWatch {
            fired_at: Some(finish.at),
            ..request
        };
        assert_eq!(shown.fired, Some(used.clone()));

        let (resent, changed) = state.merge_preferences(&chosen);
        assert!(!changed);
        assert_eq!(resent.fired_watches(), [used]);
    }

    #[test]
    fn used_watches_known_locally_and_by_the_server_are_listed_once() {
        let since = Utc::now();
        let watch = |pipeline_id: &str, since| OnceWatch {
            pipeline_id: pipeline_id.to_owned(),
            since,
            fired_at: Some(since),
        };
        let later = since + Span::seconds(1);

        let merged = used_watches(
            vec![watch("b", since), watch("a", since)],
            vec![watch("a", since), watch("a", later)],
        );

        assert_eq!(
            merged
                .iter()
                .map(|used| (used.pipeline_id.as_str(), used.since))
                .collect::<Vec<_>>(),
            [("a", since), ("a", later), ("b", since)]
        );
    }

    #[test]
    fn the_server_answer_lists_used_watches_and_anything_else_is_a_failed_send() {
        let body = r#"{"fired":[{"pipeline_id":"github__1__api","since":"2026-11-03T00:00:00Z","fired_at":"2026-11-03T00:05:00Z"}]}"#;

        let saved = parse_answer::<SavedPreferences>(200, body).unwrap();
        assert_eq!(saved.fired.len(), 1);
        assert_eq!(saved.fired[0].pipeline_id, "github__1__api");
        assert!(saved.fired[0].fired_at.is_some());
        assert!(parse_answer::<SavedPreferences>(500, body).is_none());
        assert!(parse_answer::<SavedPreferences>(200, r#"{"fired":["github__1__api"]}"#).is_none());
        assert!(
            parse_answer::<ServerPreferences>(200, r#"{"preferences":null}"#)
                .unwrap()
                .preferences
                .is_none()
        );
    }
}
