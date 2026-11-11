mod pipelines;
mod scan;
mod selection;
pub use self::selection::wanted_notifications;
#[cfg(test)]
mod tests;

use std::collections::{
    BTreeMap,
    HashSet,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

pub use self::scan::refresh_notifications;
use super::alert_candidates::seeded_keys;
use super::alerts::{
    Alert,
    AlertTracker,
};
use super::{
    connection_subject,
    instance_subject,
};
use crate::domain::{
    AlertKind,
    HealthLevel,
    NotificationPreferences,
    OnceWatch,
    Pipeline,
    PipelineStatus,
    ProviderSummary,
    RunFinish,
    SignalKind,
};
use crate::event::{
    AlertLink,
    CoreEvent,
};

const KEYS_LIMIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNotification {
    pub key: String,
    pub title: String,
    pub body: String,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<i64>,
    #[serde(default)]
    pub provider_ids: Vec<i64>,
    pub kind: AlertKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<RunFinish>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<HealthLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<AlertLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedNotification {
    pub id: u64,
    pub at: DateTime<Utc>,
    pub notice: LocalNotification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShownNotification {
    pub id: u64,
    pub at: DateTime<Utc>,
    pub tone: &'static str,
    #[serde(flatten)]
    pub notice: LocalNotification,
    pub fired: Option<OnceWatch>,
}

impl ObservedNotification {
    pub fn admit(&self, preferences: &mut NotificationPreferences) -> Option<ShownNotification> {
        let notice = &self.notice;
        if !preferences.allows(
            notice.kind,
            &notice.provider_ids,
            notice.pipeline_id.as_deref(),
            notice.finish.as_ref(),
        ) {
            return None;
        }
        let fired = notice
            .pipeline_id
            .as_deref()
            .zip(notice.finish.as_ref())
            .and_then(|(pipeline_id, finish)| {
                preferences
                    .fire_once(pipeline_id, finish)
                    .into_iter()
                    .next()
            });

        Some(ShownNotification {
            id: self.id,
            at: self.at,
            tone: notice.kind.tone(),
            notice: notice.clone(),
            fired,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PipelineMark {
    status: PipelineStatus,
    last_run: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    overdue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    awaiting_success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    finished_at: Option<DateTime<Utc>>,
}

impl PipelineMark {
    fn of(pipeline: &Pipeline) -> Self {
        Self {
            status: pipeline.status.clone(),
            last_run: pipeline.last_run,
            overdue: pipeline.overdue_run.clone(),
            checked_at: None,
            awaiting_success: pipeline.status == PipelineStatus::Failed,
            finished_at: None,
        }
    }

    fn finish_of(&self, pipeline: &Pipeline) -> Option<RunFinish> {
        pipeline
            .finished_since(&self.status, self.last_run)
            .then(|| RunFinish {
                at: Utc::now(),
                previous: self.finished_at,
            })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct ProviderMark {
    name: String,
    version: Option<i64>,
    failures: u32,
    paused_until: Option<DateTime<Utc>>,
    attempted_at: Option<DateTime<Utc>>,
    overdue_at: Option<DateTime<Utc>>,
    feed: Option<String>,
    feed_seed: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Delivered {
    keys: Vec<String>,
    recovered: bool,
    providers: Vec<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationState {
    pipelines: BTreeMap<i64, BTreeMap<String, PipelineMark>>,
    providers: BTreeMap<i64, ProviderMark>,
    alerts: AlertTracker,
    delivered: BTreeMap<String, Delivered>,
}

impl NotificationState {
    pub fn seed(&mut self, pipelines: &[Pipeline], providers: &[ProviderSummary]) {
        *self = Self::default();
        for provider in providers {
            self.track_provider(provider);
        }
        for pipeline in pipelines {
            self.pipelines
                .entry(pipeline.provider_id)
                .or_default()
                .insert(pipeline.id.clone(), PipelineMark::of(pipeline));
        }
    }

    pub fn observe_event(&mut self, event: &CoreEvent) -> Vec<LocalNotification> {
        match event {
            CoreEvent::PipelinesUpdated {
                pipelines,
                provider_id,
                ..
            } => self.observe_pipelines(*provider_id, pipelines),
            CoreEvent::PipelinesFailed { pipelines } => self.observe_failed(pipelines),
            CoreEvent::ProviderHealthAlert {
                alert_id,
                alert_key,
                notification_key,
                provider_ids,
                level,
                title,
                body,
                link,
                recovered,
            } => {
                if *recovered {
                    self.alerts.reset(notification_key);
                } else {
                    self.alerts.arm(notification_key);
                }
                self.deliver(Alert {
                    alert_id: alert_id.clone(),
                    alert_key: alert_key.clone(),
                    notification_key: notification_key.clone(),
                    provider_ids: provider_ids.clone(),
                    level: *level,
                    title: title.clone(),
                    body: body.clone(),
                    link: link.clone(),
                    recovered: *recovered,
                })
                .into_iter()
                .collect()
            }
            CoreEvent::ProviderAdded { provider, .. }
            | CoreEvent::ProviderUpdated { provider, .. }
            | CoreEvent::ProviderStatusUpdated { provider } => {
                self.track_provider(provider);
                Vec::new()
            }
            CoreEvent::ProviderRemoved { provider, .. } => {
                let remaining: HashSet<i64> = self
                    .providers
                    .keys()
                    .chain(self.pipelines.keys())
                    .copied()
                    .filter(|id| *id != provider.id)
                    .collect();
                self.retain_providers(&remaining);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    pub fn retain_providers(&mut self, ids: &HashSet<i64>) {
        self.pipelines.retain(|id, _| ids.contains(id));
        self.providers.retain(|id, _| ids.contains(id));

        let mut kept: HashSet<String> = ids
            .iter()
            .flat_map(|id| [connection_subject(*id), instance_subject(*id)])
            .collect();
        kept.extend(self.providers.values().filter_map(|mark| mark.feed.clone()));
        kept.extend(
            self.delivered
                .iter()
                .filter(|(_, memo)| memo.providers.iter().any(|id| ids.contains(id)))
                .map(|(subject, _)| subject.clone()),
        );
        self.alerts.retain(|subject| kept.contains(subject));
        self.delivered.retain(|subject, _| kept.contains(subject));
    }

    fn enroll(&mut self, id: i64, name: &str, version: Option<i64>) -> bool {
        let renewed = self
            .providers
            .get(&id)
            .is_none_or(|mark| mark.version != version);
        if renewed {
            self.alerts.close(&instance_subject(id));
            self.alerts.reset(&connection_subject(id));
        }
        let mark = self.providers.entry(id).or_default();
        if renewed {
            mark.version = version;
            mark.failures = 0;
            mark.paused_until = None;
        }
        if mark.name != name {
            mark.name = name.to_owned();
        }

        renewed
    }

    fn track_provider(&mut self, provider: &ProviderSummary) {
        if !self.enroll(provider.id, &provider.name, Some(provider.version)) {
            return;
        }
        let keys = |kind: SignalKind| {
            provider
                .health
                .signals
                .iter()
                .find(|signal| signal.kind == kind)
                .map(seeded_keys)
                .unwrap_or_default()
        };
        let down = keys(SignalKind::Connection);
        if down.is_empty() {
            self.alerts.reset(&connection_subject(provider.id));
        } else {
            self.alerts.baseline(&connection_subject(provider.id), down);
        }
        self.alerts
            .baseline(&instance_subject(provider.id), keys(SignalKind::Instance));
        if let Some(mark) = self.providers.get_mut(&provider.id) {
            mark.feed_seed = provider
                .health
                .status_page
                .is_some()
                .then(|| keys(SignalKind::StatusPage));
        }
    }

    fn deliver(&mut self, alert: Alert) -> Option<LocalNotification> {
        let memo = self
            .delivered
            .entry(alert.notification_key.clone())
            .or_default();
        memo.providers = alert.provider_ids.clone();
        let duplicate = if alert.recovered {
            memo.recovered
        } else {
            memo.keys.contains(&alert.alert_key)
        };
        if duplicate {
            return None;
        }
        if alert.recovered {
            memo.keys.clear();
        } else {
            if memo.keys.len() == KEYS_LIMIT {
                memo.keys.remove(0);
            }
            memo.keys.push(alert.alert_key);
        }
        memo.recovered = alert.recovered;

        Some(LocalNotification {
            key: alert.notification_key,
            title: alert.title,
            body: alert.body,
            pipeline_id: None,
            provider_id: alert.provider_ids.first().copied(),
            provider_ids: alert.provider_ids,
            kind: if alert.recovered {
                AlertKind::Recovered
            } else {
                AlertKind::Outage
            },
            finish: None,
            level: Some(alert.level),
            link: alert.link,
        })
    }

    fn deliver_all(&mut self, alerts: Vec<Alert>) -> Vec<LocalNotification> {
        alerts
            .into_iter()
            .filter_map(|alert| self.deliver(alert))
            .collect()
    }
}
