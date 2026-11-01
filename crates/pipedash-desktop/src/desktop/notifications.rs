use pipedash_core::{
    application::services::health_service::{
        LocalNotification,
        NotificationTone,
    },
    domain::HealthLevel,
    event::AlertLink,
};
use serde::Serialize;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
};
use tauri_plugin_pipedash_native::{
    BuildWatchRunStatus,
    BuildWatchSnapshot,
};

use super::{
    DesktopState,
    Ticket,
};
use crate::native::NotificationTarget;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopNotice {
    pub key: String,
    pub at: i64,
    pub read: bool,
    pub tone: NotificationTone,
    pub level: Option<HealthLevel>,
    pub title: String,
    pub body: String,
    pub provider_ids: Vec<i64>,
    pub pipeline_id: Option<String>,
    pub link: Option<AlertLink>,
    pub run_number: Option<i64>,
}

impl From<LocalNotification> for DesktopNotice {
    fn from(notice: LocalNotification) -> Self {
        Self {
            key: notice.key,
            at: chrono::Utc::now().timestamp_millis(),
            read: false,
            tone: notice.tone,
            level: notice.level,
            title: notice.title,
            body: notice.body,
            provider_ids: notice.provider_ids,
            pipeline_id: notice.pipeline_id,
            link: notice.link,
            run_number: None,
        }
    }
}

impl DesktopNotice {
    pub fn watched(watch: &BuildWatchSnapshot) -> Self {
        let (tone, result) = match watch.status {
            BuildWatchRunStatus::Success => (NotificationTone::Success, "succeeded"),
            BuildWatchRunStatus::Failed => (NotificationTone::Danger, "failed"),
            BuildWatchRunStatus::Cancelled => (NotificationTone::Warning, "was cancelled"),
            BuildWatchRunStatus::Skipped => (NotificationTone::Warning, "was skipped"),
            BuildWatchRunStatus::Pending | BuildWatchRunStatus::Running => {
                (NotificationTone::Warning, "finished")
            }
        };
        Self {
            key: format!("watch:{}", watch.session_id),
            at: chrono::Utc::now().timestamp_millis(),
            read: false,
            tone,
            level: None,
            title: format!("Watched run #{} {result}", watch.run_number),
            body: format!("{} · {}", watch.pipeline_name, watch.provider_name),
            provider_ids: Vec::new(),
            pipeline_id: Some(watch.pipeline_id.clone()),
            link: None,
            run_number: Some(watch.run_number),
        }
    }

    fn target(&self) -> NotificationTarget {
        NotificationTarget {
            key: Some(self.key.clone()),
            pipeline_id: self.pipeline_id.clone(),
            provider_id: self
                .provider_ids
                .first()
                .and_then(|id| u64::try_from(*id).ok()),
            run_number: self
                .run_number
                .and_then(|number| u64::try_from(number).ok()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DesktopNotificationSnapshot {
    pub revision: u64,
    pub items: Vec<DesktopNotice>,
}

impl DesktopNotificationSnapshot {
    pub fn push(&mut self, notice: DesktopNotice) {
        self.items.retain(|existing| existing.key != notice.key);
        self.items.insert(0, notice);
        self.items.truncate(50);
        self.revision += 1;
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.revision += 1;
    }

    pub fn mark_read(&mut self, key: Option<&str>) {
        let mut changed = false;
        for item in &mut self.items {
            if !item.read && key.is_none_or(|key| key == item.key) {
                item.read = true;
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
    }
}

pub(super) fn publish<R: Runtime>(app: &AppHandle<R>, snapshot: &DesktopNotificationSnapshot) {
    if let Err(error) = app.emit("desktop-notifications-changed", snapshot) {
        tracing::warn!(%error, "Couldn't publish desktop notifications");
    }
}

pub(super) fn deliver<R: Runtime>(app: &AppHandle<R>, ticket: Ticket, notices: &[DesktopNotice]) {
    for notice in notices {
        let allowed = {
            let state = app.state::<DesktopState>();
            let inner = state.lock();
            inner.matches(ticket)
                && inner.baseline.is_some()
                && inner.saved.settings.notifications_enabled
        };
        if !allowed {
            break;
        }
        if let Err(error) = crate::system_notifications::send(
            app,
            &notice.title,
            &notice.body,
            Some(&notice.target()),
        ) {
            tracing::warn!(error = %error.error, "Desktop notification submission failed");
        }
    }
}

#[tauri::command]
pub fn get_desktop_notifications(app: AppHandle) -> DesktopNotificationSnapshot {
    app.state::<DesktopState>().lock().history.clone()
}

#[tauri::command]
pub fn mark_desktop_notifications_read(
    app: AppHandle, key: Option<String>,
) -> DesktopNotificationSnapshot {
    let snapshot = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.history.mark_read(key.as_deref());
        inner.history.clone()
    };
    publish(&app, &snapshot);
    snapshot
}

#[tauri::command]
pub fn clear_desktop_notifications(app: AppHandle) -> DesktopNotificationSnapshot {
    let snapshot = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.history.clear();
        inner.history.clone()
    };
    publish(&app, &snapshot);
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(key: &str) -> DesktopNotice {
        DesktopNotice {
            key: key.to_string(),
            at: 1,
            read: false,
            tone: NotificationTone::Warning,
            level: None,
            title: key.to_string(),
            body: String::new(),
            provider_ids: Vec::new(),
            pipeline_id: None,
            link: None,
            run_number: None,
        }
    }

    #[test]
    fn history_keeps_fifty_recent_keys_and_replaces_read_entries() {
        let mut history = DesktopNotificationSnapshot::default();
        for index in 0..60 {
            history.push(notice(&index.to_string()));
        }
        assert_eq!(history.items.len(), 50);
        assert_eq!(history.items.last().unwrap().key, "10");
        history.mark_read(Some("11"));
        assert!(
            history
                .items
                .iter()
                .find(|item| item.key == "11")
                .unwrap()
                .read
        );
        history.push(notice("11"));
        assert_eq!(history.items[0].key, "11");
        assert!(!history.items[0].read);
        assert_eq!(
            history.items.iter().filter(|item| item.key == "11").count(),
            1
        );
        let revision = history.revision;
        history.mark_read(Some("missing"));
        assert_eq!(history.revision, revision);
        history.clear();
        assert!(history.items.is_empty());
        assert!(history.revision > revision);
    }
}
