#[cfg(mobile)]
use std::collections::HashSet;
use std::sync::atomic::Ordering;
#[cfg(mobile)]
use std::sync::Arc;

use pipedash_core::application::{
    wanted_notifications,
    LocalNotification,
    NotificationState,
};
use pipedash_core::domain::{
    NotificationPreferences,
    RunFinish,
};
#[cfg(mobile)]
use pipedash_core::event::CoreEvent;
use tauri::{
    AppHandle,
    Manager,
    Runtime,
};

use super::preferences::remember_finishes;
use super::settings::permission_granted;
#[cfg(mobile)]
use super::settings::{
    initialize,
    schedule_sync,
};
use super::store;
use super::{
    error,
    storage,
    LocalNotificationsState,
    Result,
};
use crate::native::NotificationTarget;

pub(super) async fn deliver<R: Runtime>(
    app: &AppHandle<R>, preferences: &NotificationPreferences, notices: Vec<LocalNotification>,
) -> Result<Vec<(String, RunFinish)>> {
    let notices = wanted_notifications(preferences, notices);
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
pub(crate) async fn on_event<R: Runtime>(app: &AppHandle<R>, event: &CoreEvent) {
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
pub(super) async fn observe<R: Runtime>(
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
pub(super) async fn observe<R: Runtime>(
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
