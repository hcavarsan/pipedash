use std::sync::atomic::Ordering;
use std::sync::Arc;

use pipedash_core::application::NotificationState;
use tauri::{
    AppHandle,
    Manager,
    Runtime,
};
use tauri_plugin_pipedash_native::{
    NotificationRefresh,
    PipedashNativeExt,
};
use tokio::sync::watch;

use super::events::deliver;
use super::preferences::{
    remember_finishes,
    retry_host_preferences,
};
use super::settings::{
    initialize,
    schedule_sync,
    sync_worker,
};
use super::store;
use super::{
    error,
    storage,
    wake,
    LocalNotificationsState,
    Result,
    HOST_PREFERENCES_PENDING,
    PREFERENCES_SYNC,
    REFRESH_TIMEOUT,
};

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
        wake::scan_providers(authorization.as_ref(), &mut observations),
        wake::fetch_hosted(host.as_ref()),
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

async fn finish_native<R: Runtime>(app: &AppHandle<R>, id: &str, success: bool) {
    if app
        .pipedash_native()
        .finish_notification_refresh(id, success)
        .await
        .is_err()
    {
        tracing::warn!("Could not complete the native notification refresh task");
    }
}

async fn run_refresh<R: Runtime>(
    app: AppHandle<R>, id: String, mut cancelled: watch::Receiver<bool>,
) {
    let success = tokio::select! {
        biased;
        _ = cancelled.changed() => false,
        result = tokio::time::timeout(REFRESH_TIMEOUT, refresh(&app)) => matches!(result, Ok(Ok(()))),
    };
    app.state::<LocalNotificationsState>()
        .cancel_refresh(Some(&id));
    finish_native(&app, &id, success).await;
}

async fn start<R: Runtime>(app: AppHandle<R>) {
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
}

pub(crate) fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
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
                return;
            }
            let admitted = handle
                .state::<LocalNotificationsState>()
                .start_refresh(&event.id);
            let handle = handle.clone();
            match admitted {
                Some(cancelled) => {
                    tauri::async_runtime::spawn(run_refresh(handle, event.id, cancelled));
                }
                None => {
                    tauri::async_runtime::spawn(async move {
                        finish_native(&handle, &event.id, false).await;
                    });
                }
            }
        })
        .map_err(|failure| tauri::Error::Anyhow(failure.into()))?;
    tauri::async_runtime::spawn(sync_worker(app.clone()));
    tauri::async_runtime::spawn(start(app.clone()));
    Ok(())
}
