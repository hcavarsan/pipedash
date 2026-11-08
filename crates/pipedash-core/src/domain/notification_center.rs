use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

use super::{
    AlertKind,
    DomainError,
    DomainResult,
    HealthLevel,
};
use crate::application::{
    LocalNotification,
    ShownNotification,
};
use crate::event::AlertLink;

pub const MAX_CENTER_NOTIFICATIONS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CenterTone {
    Danger,
    Warning,
    Success,
}

impl CenterTone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Danger => "danger",
            Self::Warning => "warning",
            Self::Success => "success",
        }
    }
}

impl From<AlertKind> for CenterTone {
    fn from(kind: AlertKind) -> Self {
        match kind {
            AlertKind::Failed | AlertKind::Degraded => Self::Danger,
            AlertKind::LongRun | AlertKind::Outage => Self::Warning,
            AlertKind::Recovered | AlertKind::Finished | AlertKind::Test => Self::Success,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchOutcome {
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    Finished,
}

impl WatchOutcome {
    fn tone(self) -> CenterTone {
        match self {
            Self::Succeeded => CenterTone::Success,
            Self::Failed => CenterTone::Danger,
            Self::Cancelled | Self::Skipped | Self::Finished => CenterTone::Warning,
        }
    }

    fn result(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "was cancelled",
            Self::Skipped => "was skipped",
            Self::Finished => "finished",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CenterNotification {
    pub key: String,
    pub at: i64,
    pub read: bool,
    pub tone: CenterTone,
    pub level: Option<HealthLevel>,
    pub title: String,
    pub body: String,
    pub provider_ids: Vec<i64>,
    pub pipeline_id: Option<String>,
    pub link: Option<AlertLink>,
    pub run_number: Option<i64>,
}

impl CenterNotification {
    fn shown(at: DateTime<Utc>, notice: &LocalNotification) -> Self {
        Self {
            key: notice.key.clone(),
            at: at.timestamp_millis(),
            read: false,
            tone: notice.kind.into(),
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

    pub fn watched(
        session_id: &str, run_number: i64, pipeline_id: &str, pipeline_name: &str,
        provider_name: &str, outcome: WatchOutcome,
    ) -> Self {
        Self {
            key: format!("watch:{session_id}"),
            at: Utc::now().timestamp_millis(),
            read: false,
            tone: outcome.tone(),
            level: None,
            title: format!("Watched run #{run_number} {}", outcome.result()),
            body: format!("{pipeline_name} · {provider_name}"),
            provider_ids: Vec::new(),
            pipeline_id: Some(pipeline_id.to_owned()),
            link: None,
            run_number: Some(run_number),
        }
    }
}

impl From<&ShownNotification> for CenterNotification {
    fn from(shown: &ShownNotification) -> Self {
        Self::shown(shown.at, &shown.notice)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationCenterSnapshot {
    pub revision: u64,
    pub items: Vec<CenterNotification>,
    #[serde(default)]
    pub unread: usize,
}

impl NotificationCenterSnapshot {
    fn count_unread(&self) -> usize {
        self.items.iter().filter(|item| !item.read).count()
    }

    fn commit(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.unread = self.count_unread();
    }

    pub fn push(&mut self, notice: CenterNotification) {
        self.items.retain(|existing| existing.key != notice.key);
        self.items.insert(0, notice);
        self.items.truncate(MAX_CENTER_NOTIFICATIONS);
        self.commit();
    }

    pub fn insert_missed(&mut self, notice: CenterNotification) -> bool {
        if self
            .items
            .iter()
            .any(|existing| existing.key == notice.key && existing.at >= notice.at)
        {
            return false;
        }
        self.items.retain(|existing| existing.key != notice.key);
        let index = self
            .items
            .iter()
            .position(|existing| existing.at < notice.at)
            .unwrap_or(self.items.len());
        if index >= MAX_CENTER_NOTIFICATIONS {
            return false;
        }
        self.items.insert(index, notice);
        self.items.truncate(MAX_CENTER_NOTIFICATIONS);
        self.commit();
        true
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.commit();
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
            self.commit();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationCenterChange {
    Push { notice: CenterNotification },
    InsertMissed { notice: CenterNotification },
    Read { key: Option<String> },
    Clear,
}

pub fn update_notification_center(
    mut snapshot: NotificationCenterSnapshot, change: NotificationCenterChange,
) -> DomainResult<NotificationCenterSnapshot> {
    if snapshot.items.len() > MAX_CENTER_NOTIFICATIONS {
        return Err(DomainError::InvalidConfig(format!(
            "Notification center holds at most {MAX_CENTER_NOTIFICATIONS} items"
        )));
    }
    snapshot.unread = snapshot.count_unread();
    match change {
        NotificationCenterChange::Push { notice } => snapshot.push(notice),
        NotificationCenterChange::InsertMissed { notice } => {
            snapshot.insert_missed(notice);
        }
        NotificationCenterChange::Read { key } => snapshot.mark_read(key.as_deref()),
        NotificationCenterChange::Clear => snapshot.clear(),
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests;
