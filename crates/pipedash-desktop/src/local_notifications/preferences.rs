use std::path::Path;
use std::sync::atomic::Ordering;

use pipedash_core::domain::{
    NotificationPreferences,
    OnceWatch,
    RunFinish,
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
};
use tauri::AppHandle;

use super::store::{
    self,
    AlertHost,
};
use super::{
    storage,
    LocalNotificationsState,
    Result,
    HOST_PREFERENCES_PENDING,
    PREFERENCES_RETRY_BUDGET,
    PREFERENCES_SYNC,
};
use crate::remote::alerts::{
    client_request,
    host_request,
};
use crate::remote::connection::RemoteConnection;
use crate::remote::model::RemoteError;

pub(super) const PREFERENCES_PATH: &str = "/devices/current/notification-preferences";

#[derive(Deserialize)]
pub(super) struct SavedPreferences {
    #[serde(default)]
    pub fired: Vec<OnceWatch>,
}

#[derive(Deserialize)]
pub(super) struct ServerPreferences {
    pub preferences: Option<NotificationPreferences>,
}

pub(super) enum Server<'a> {
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

    async fn set_pending(&self, pending: bool) {
        match self {
            Self::Host(_) => HOST_PREFERENCES_PENDING.store(pending, Ordering::Release),
            Self::Client(remote, dir) => remote.set_preferences_pending(dir, pending).await,
        }
    }
}

pub(super) fn parse_answer<T: DeserializeOwned>(status: u16, body: &str) -> Option<T> {
    (status == 200)
        .then(|| serde_json::from_str(body).ok())
        .flatten()
}

pub(super) async fn remember_finishes(finishes: Vec<(String, RunFinish)>) {
    if finishes.is_empty() {
        return;
    }
    let _sync = PREFERENCES_SYNC.lock().await;
    if storage(move || store::fire_once(&finishes)).await.is_err() {
        tracing::warn!("Couldn't mark a one-time watch as used");
    }
}

pub(super) async fn send_preferences(
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
    server.set_pending(saved.is_none()).await;

    saved.map(|saved| saved.fired).unwrap_or_default()
}

pub(super) async fn sync_host_preferences(host: &AlertHost) {
    let _sync = PREFERENCES_SYNC.lock().await;
    if let Ok(preferences) = storage(store::preferences).await {
        send_preferences(&Server::Host(host), &preferences).await;
    }
}

pub(super) async fn retry_host_preferences(host: &AlertHost) {
    if HOST_PREFERENCES_PENDING.load(Ordering::Acquire)
        && tokio::time::timeout(PREFERENCES_RETRY_BUDGET, sync_host_preferences(host))
            .await
            .is_err()
    {
        tracing::debug!("The alert host took too long to take the notification preferences");
    }
}

fn preferences_changed(app: &AppHandle) {
    #[cfg(desktop)]
    crate::desktop::tray_changed(app);
    #[cfg(not(desktop))]
    let _ = app;
}

pub(super) async fn merge_local(
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
