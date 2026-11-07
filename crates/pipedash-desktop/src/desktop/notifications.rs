use chrono::{
    DateTime,
    Utc,
};
use pipedash_core::{
    application::{
        LocalNotification,
        ShownNotification,
    },
    domain::{
        AlertKind,
        HealthLevel,
    },
    event::AlertLink,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
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
    Origin,
};
use crate::native::NotificationTarget;
use crate::remote::alerts::HostAlert;
use crate::remote::RemoteConnection;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopNotice {
    pub key: String,
    pub at: i64,
    pub read: bool,
    pub tone: &'static str,
    pub level: Option<HealthLevel>,
    pub title: String,
    pub body: String,
    pub provider_ids: Vec<i64>,
    pub pipeline_id: Option<String>,
    pub link: Option<AlertLink>,
    pub run_number: Option<i64>,
}

impl DesktopNotice {
    fn shown(at: DateTime<Utc>, notice: &LocalNotification) -> Self {
        Self {
            key: notice.key.clone(),
            at: at.timestamp_millis(),
            read: false,
            tone: notice.kind.tone(),
            level: notice.level,
            title: notice.title.clone(),
            body: notice.body.clone(),
            provider_ids: notice.provider_ids.clone(),
            pipeline_id: notice.pipeline_id.clone(),
            link: notice.link.clone(),
            run_number: None,
        }
    }

    pub fn received(payload: &Value) -> Option<Self> {
        #[derive(Deserialize)]
        struct Received {
            at: DateTime<Utc>,
            #[serde(flatten)]
            notice: LocalNotification,
        }

        let received = Received::deserialize(payload).ok()?;

        Some(Self::shown(received.at, &received.notice))
    }

    pub fn missed(alert: &HostAlert) -> Option<Self> {
        Some(Self {
            key: alert.key.clone(),
            at: DateTime::parse_from_rfc3339(&alert.created_at)
                .ok()?
                .timestamp_millis(),
            read: false,
            tone: AlertKind::parse(&alert.kind)?.tone(),
            level: None,
            title: alert.title.clone(),
            body: alert.body.clone(),
            provider_ids: if alert.provider_ids.is_empty() {
                alert.provider_id.into_iter().collect()
            } else {
                alert.provider_ids.clone()
            },
            pipeline_id: alert.pipeline_id.clone(),
            link: None,
            run_number: None,
        })
    }

    pub fn watched(watch: &BuildWatchSnapshot) -> Self {
        let (tone, result) = match watch.status {
            BuildWatchRunStatus::Success => ("success", "succeeded"),
            BuildWatchRunStatus::Failed => ("danger", "failed"),
            BuildWatchRunStatus::Cancelled => ("warning", "was cancelled"),
            BuildWatchRunStatus::Skipped => ("warning", "was skipped"),
            BuildWatchRunStatus::Pending | BuildWatchRunStatus::Running => ("warning", "finished"),
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

impl From<&ShownNotification> for DesktopNotice {
    fn from(shown: &ShownNotification) -> Self {
        Self::shown(shown.at, &shown.notice)
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

pub(super) fn announce<R: Runtime>(
    app: &AppHandle<R>, notice: &DesktopNotice, history: &DesktopNotificationSnapshot,
    deliver: bool,
) {
    publish(app, history);
    if !deliver {
        return;
    }
    if let Err(error) =
        crate::system_notifications::send(app, &notice.title, &notice.body, Some(&notice.target()))
    {
        tracing::warn!(error = %error.error, "Desktop notification submission failed");
    }
}

fn record<R: Runtime>(
    app: &AppHandle<R>, origin: Origin, notices: impl IntoIterator<Item = DesktopNotice>,
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
        for notice in std::iter::once(first).chain(notices) {
            inner.history.push(notice);
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
        super::wake(app);
    }
}

pub fn record_shown<R: Runtime>(app: &AppHandle<R>, generation: u64, shown: &ShownNotification) {
    record(
        app,
        Origin::Core(generation),
        [DesktopNotice::from(shown)],
        true,
    );
}

pub fn record_received<R: Runtime>(app: &AppHandle<R>, payload: &Value) {
    if let Some(notice) = DesktopNotice::received(payload) {
        record(app, Origin::Server, [notice], true);
    }
}

pub fn record_missed<R: Runtime>(app: &AppHandle<R>, alerts: &[HostAlert]) {
    record(
        app,
        Origin::Server,
        alerts.iter().filter_map(DesktopNotice::missed),
        false,
    );
}

#[tauri::command]
pub fn get_desktop_notifications(app: AppHandle) -> DesktopNotificationSnapshot {
    app.state::<DesktopState>().lock().history.clone()
}

pub fn mark_read<R: Runtime>(app: &AppHandle<R>, key: Option<&str>) -> DesktopNotificationSnapshot {
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
) -> DesktopNotificationSnapshot {
    mark_read(&app, key.as_deref())
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
    super::tray_changed(&app);
    snapshot
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use pipedash_core::{
        domain::AlertKind,
        event::AlertLinkKind,
    };

    use super::*;

    fn notice(key: &str) -> DesktopNotice {
        DesktopNotice {
            key: key.to_string(),
            at: 1,
            read: false,
            tone: "warning",
            level: None,
            title: key.to_string(),
            body: String::new(),
            provider_ids: Vec::new(),
            pipeline_id: None,
            link: None,
            run_number: None,
        }
    }

    fn shown(kind: AlertKind) -> ShownNotification {
        ShownNotification {
            id: 3,
            at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            tone: kind.tone(),
            notice: LocalNotification {
                key: "provider:1".to_string(),
                title: "Provider health".to_string(),
                body: "API down".to_string(),
                pipeline_id: None,
                provider_id: Some(1),
                provider_ids: vec![1, 2],
                kind,
                finish: None,
                level: Some(HealthLevel::MajorOutage),
                link: Some(AlertLink {
                    url: "https://status.example.com".to_string(),
                    kind: AlertLinkKind::Incident,
                }),
            },
            fired: None,
        }
    }

    #[test]
    fn a_shown_notification_keeps_its_metadata_and_takes_its_tone_from_the_kind() {
        for (kind, tone) in [
            (AlertKind::Failed, "danger"),
            (AlertKind::Outage, "warning"),
            (AlertKind::Recovered, "success"),
        ] {
            let source = shown(kind);
            let notice = DesktopNotice::from(&source);

            assert_eq!(notice.tone, tone);
            assert_eq!(notice.key, "provider:1");
            assert_eq!(notice.at, 1_700_000_000_000);
            assert!(!notice.read);
            assert_eq!(notice.level, Some(HealthLevel::MajorOutage));
            assert_eq!(notice.provider_ids, vec![1, 2]);
            assert_eq!(notice.link, source.notice.link);
            assert_eq!(notice.run_number, None);
        }
    }

    #[test]
    fn a_notification_frame_from_a_server_maps_like_the_local_one() {
        let source = shown(AlertKind::Degraded);
        let payload = serde_json::to_value(&source).unwrap();

        let received = DesktopNotice::received(&payload).unwrap();
        let local = DesktopNotice::from(&source);

        assert_eq!(received.key, local.key);
        assert_eq!(received.at, local.at);
        assert_eq!(received.tone, "danger");
        assert_eq!(received.title, local.title);
        assert_eq!(received.provider_ids, local.provider_ids);
        assert_eq!(received.link, local.link);
        assert!(DesktopNotice::received(&serde_json::json!({ "key": "x" })).is_none());
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
