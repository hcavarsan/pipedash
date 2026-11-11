use std::sync::atomic::Ordering;
use std::sync::{
    Arc,
    PoisonError,
};

use chrono::Utc;
use pipedash_core::application::NotificationState;
use pipedash_core::domain::{
    change_preferences,
    merge_fired_watches,
    NotificationPreferenceSnapshot,
    NotificationPreferences,
    OnceWatch,
    PreferenceChange,
};
use serde::Serialize;
use tauri::{
    AppHandle,
    Manager,
    Runtime,
    State,
};
use tauri_plugin_pipedash_native::{
    LocalNotificationSettings,
    PipedashNativeExt,
};

use super::preferences::{
    merge_local,
    parse_answer,
    send_preferences,
    sync_host_preferences,
    Server,
    ServerPreferences,
    PREFERENCES_PATH,
};
use super::settings::{
    activate,
    capture_authorization,
    current_alert_host,
    detach_alert_host,
    disable,
    enable_push,
    initialize,
    permission_granted,
    set_push_problem,
    unlocked_core,
};
use super::store::{
    self,
    AlertHost,
};
use super::{
    error,
    storage,
    LocalNotificationsState,
    Result,
    HOST_PREFERENCES_PENDING,
    PREFERENCES_SYNC,
};
use crate::remote::alerts::{
    claim_alert_host,
    client_alert_host,
    client_request,
    revoke_device,
};
use crate::remote::connection::RemoteConnection;
use crate::remote::model::RemoteError;
use crate::remote::pairing::parse_pairing;
use crate::AppDataDir;

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

async fn apply_preferences(
    app: &AppHandle, state: &LocalNotificationsState, remote: &RemoteConnection,
    data_dir: &AppDataDir, preferences: NotificationPreferences,
) -> Result<(NotificationPreferences, Vec<OnceWatch>)> {
    let (preferences, changed) = merge_local(app, state, preferences).await?;
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
            Some(Server::Client(remote, &data_dir.0))
        } else {
            host.as_ref().map(Server::Host)
        };
        match server {
            Some(server) => sent = send_preferences(&server, &preferences).await,
            None => HOST_PREFERENCES_PENDING.store(false, Ordering::Release),
        }
    }

    Ok((preferences, sent))
}

#[tauri::command]
pub async fn set_notification_preferences(
    app: AppHandle, state: State<'_, LocalNotificationsState>, remote: State<'_, RemoteConnection>,
    data_dir: State<'_, AppDataDir>, preferences: NotificationPreferences,
) -> Result<Vec<OnceWatch>> {
    let _sync = PREFERENCES_SYNC.lock().await;
    let (preferences, sent) =
        apply_preferences(&app, &state, &remote, &data_dir, preferences).await?;

    Ok(merge_fired_watches(preferences.fired_watches(), sent))
}

#[tauri::command]
pub async fn change_notification_preference(
    app: AppHandle, state: State<'_, LocalNotificationsState>, remote: State<'_, RemoteConnection>,
    data_dir: State<'_, AppDataDir>, change: PreferenceChange,
) -> Result<NotificationPreferenceSnapshot> {
    let _sync = PREFERENCES_SYNC.lock().await;
    let mut next = state.preferences().clone();
    change_preferences(&mut next, change, Utc::now())?;
    let (preferences, _) = apply_preferences(&app, &state, &remote, &data_dir, next).await?;

    Ok(NotificationPreferenceSnapshot::new(preferences))
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
