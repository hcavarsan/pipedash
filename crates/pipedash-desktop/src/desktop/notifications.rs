use chrono::DateTime;
use pipedash_core::{
    application::ShownNotification,
    domain::{
        notification_center::{
            CenterNotification,
            NotificationCenterSnapshot,
            WatchOutcome,
        },
        AlertKind,
    },
};
use pipedash_plugin_api::build_watch::{
    BuildWatchRunStatus,
    BuildWatchSnapshot,
};
use serde_json::Value;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
};

use super::{
    projection::wake,
    state::{
        DesktopState,
        Origin,
    },
};
use crate::native::NotificationTarget;
use crate::remote::alerts::HostAlert;
use crate::remote::connection::RemoteConnection;

fn missed(alert: &HostAlert) -> Option<CenterNotification> {
    Some(CenterNotification {
        key: alert.key.clone(),
        at: DateTime::parse_from_rfc3339(&alert.created_at)
            .ok()?
            .timestamp_millis(),
        read: false,
        tone: AlertKind::parse(&alert.kind)?.into(),
        level: None,
        title: alert.title.clone(),
        body: alert.body.clone(),
        provider_ids: alert.affected_providers(),
        pipeline_id: alert.pipeline_id.clone(),
        link: None,
        run_number: None,
    })
}

pub(super) fn watched(watch: &BuildWatchSnapshot) -> CenterNotification {
    let outcome = match watch.status {
        BuildWatchRunStatus::Success => WatchOutcome::Succeeded,
        BuildWatchRunStatus::Failed => WatchOutcome::Failed,
        BuildWatchRunStatus::Cancelled => WatchOutcome::Cancelled,
        BuildWatchRunStatus::Skipped => WatchOutcome::Skipped,
        BuildWatchRunStatus::Pending | BuildWatchRunStatus::Running => WatchOutcome::Finished,
    };
    CenterNotification::watched(
        &watch.session_id,
        watch.run_number,
        &watch.pipeline_id,
        &watch.pipeline_name,
        &watch.provider_name,
        outcome,
    )
}

fn target(notice: &CenterNotification) -> NotificationTarget {
    NotificationTarget {
        key: Some(notice.key.clone()),
        pipeline_id: notice.pipeline_id.clone(),
        provider_id: notice
            .provider_ids
            .first()
            .and_then(|id| u64::try_from(*id).ok()),
        run_number: notice
            .run_number
            .and_then(|number| u64::try_from(number).ok()),
    }
}

pub(super) fn publish<R: Runtime>(app: &AppHandle<R>, snapshot: &NotificationCenterSnapshot) {
    if let Err(error) = app.emit("desktop-notifications-changed", snapshot) {
        tracing::warn!(%error, "Couldn't publish desktop notifications");
    }
}

pub(super) fn announce<R: Runtime>(
    app: &AppHandle<R>, notice: &CenterNotification, history: &NotificationCenterSnapshot,
    deliver: bool,
) {
    publish(app, history);
    if !deliver {
        return;
    }
    if let Err(error) =
        crate::system_notifications::send(app, &notice.title, &notice.body, Some(&target(notice)))
    {
        tracing::warn!(error = %error.error, "Desktop notification submission failed");
    }
}

fn record<R: Runtime>(
    app: &AppHandle<R>, origin: Origin, notices: impl IntoIterator<Item = CenterNotification>,
    notify: bool,
) {
    let mut notices = notices.into_iter();
    let Some(first) = notices.next() else {
        return;
    };
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let server = match origin {
        Origin::Server => {
            match app
                .try_state::<RemoteConnection>()
                .and_then(|remote| remote.tray_link())
            {
                Some((_, _, server)) => Some(server),
                None => return,
            }
        }
        Origin::Core(_) => None,
    };
    let (history, deliver, moved) = {
        let mut inner = state.lock();
        if !inner.accepts(origin) {
            return;
        }
        let moved = server.is_some() && inner.adopt_server(server);
        let mut changed = moved;
        for notice in std::iter::once(first).chain(notices) {
            if notify {
                inner.history.push(notice);
                changed = true;
            } else {
                changed |= inner.history.insert_missed(notice);
            }
        }
        if !changed {
            return;
        }
        (
            inner.history.clone(),
            inner.saved.settings.notifications_enabled,
            moved,
        )
    };
    announce(app, &history.items[0], &history, notify && deliver);
    super::tray_changed(app);
    if moved {
        wake(app);
    }
}

pub fn record_shown<R: Runtime>(app: &AppHandle<R>, generation: u64, shown: &ShownNotification) {
    record(
        app,
        Origin::Core(generation),
        [CenterNotification::from(shown)],
        true,
    );
}

pub fn record_received<R: Runtime>(app: &AppHandle<R>, payload: &Value) {
    if let Some(notice) = CenterNotification::received(payload) {
        record(app, Origin::Server, [notice], true);
    }
}

pub fn record_missed<R: Runtime>(app: &AppHandle<R>, alerts: &[HostAlert]) {
    record(app, Origin::Server, alerts.iter().filter_map(missed), false);
}

#[tauri::command]
pub fn get_desktop_notifications<R: Runtime>(app: AppHandle<R>) -> NotificationCenterSnapshot {
    app.state::<DesktopState>().lock().history.clone()
}

pub fn mark_read<R: Runtime>(app: &AppHandle<R>, key: Option<&str>) -> NotificationCenterSnapshot {
    let snapshot = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.history.mark_read(key);
        inner.history.clone()
    };
    publish(app, &snapshot);
    super::tray_changed(app);
    snapshot
}

#[tauri::command]
pub fn mark_desktop_notifications_read(
    app: AppHandle, key: Option<String>,
) -> NotificationCenterSnapshot {
    mark_read(&app, key.as_deref())
}

#[tauri::command]
pub fn clear_desktop_notifications(app: AppHandle) -> NotificationCenterSnapshot {
    let snapshot = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.history.clear();
        inner.history.clone()
    };
    publish(&app, &snapshot);
    super::tray_changed(&app);
    snapshot
}

#[cfg(test)]
pub(crate) fn test_app(
    remote: RemoteConnection, dir: &std::path::Path,
) -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_app();
    app.manage(DesktopState {
        inner: super::state::Inner::default().into(),
        path: dir.join("desktop.json"),
        settings_gate: Default::default(),
        projection_gate: Default::default(),
        refresh: Default::default(),
        generations: Default::default(),
        render: Box::new(|| {}),
    });
    app.manage(remote);
    app.manage(crate::AppDataDir(dir.to_path_buf()));
    app
}

#[cfg(test)]
mod tests;
