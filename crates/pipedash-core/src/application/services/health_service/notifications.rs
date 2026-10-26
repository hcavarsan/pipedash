mod scan;
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
use super::alerts::{
    seeded_keys,
    Alert,
    AlertTracker,
};
use super::{
    connection_subject,
    instance_subject,
};
use crate::domain::{
    Pipeline,
    PipelineStatus,
    ProviderSummary,
    SignalKind,
};
use crate::event::CoreEvent;

const TITLES_LIMIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNotification {
    pub key: String,
    pub title: String,
    pub body: String,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PipelineMark {
    status: PipelineStatus,
    last_run: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    overdue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checked_at: Option<DateTime<Utc>>,
}

impl PipelineMark {
    fn of(pipeline: &Pipeline) -> Self {
        Self {
            status: pipeline.status.clone(),
            last_run: pipeline.last_run,
            overdue: pipeline.overdue_run.clone(),
            checked_at: None,
        }
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
    feed: Option<String>,
    feed_seed: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Delivered {
    titles: Vec<String>,
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
                notification_key,
                provider_ids,
                title,
                body,
                recovered,
                ..
            } => {
                if *recovered {
                    self.alerts.reset(notification_key);
                } else {
                    self.alerts.arm(notification_key);
                }
                self.deliver(notification_key, title, body, provider_ids, *recovered)
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

    fn observe_pipelines(
        &mut self, provider_id: i64, pipelines: &[Pipeline],
    ) -> Vec<LocalNotification> {
        let previous = self.pipelines.remove(&provider_id).unwrap_or_default();
        let mut next = BTreeMap::new();
        let mut found = Vec::new();
        for pipeline in pipelines {
            let mut mark = PipelineMark::of(pipeline);
            if let Some(old) = previous.get(&pipeline.id) {
                let failed = pipeline.status == PipelineStatus::Failed
                    && (old.status != PipelineStatus::Failed || old.last_run != pipeline.last_run);
                if failed {
                    found.push(self.pipeline_notice(pipeline, format!("{} failed", pipeline.name)));
                }
                if pipeline.overdue_run.is_some() && old.overdue != pipeline.overdue_run {
                    found.push(self.pipeline_notice(
                        pipeline,
                        format!("{} is running longer than usual", pipeline.name),
                    ));
                }
                if mark.overdue.is_none() {
                    mark.overdue = old.overdue.clone();
                }
                mark.checked_at = old.checked_at;
            }
            next.insert(pipeline.id.clone(), mark);
        }
        if !next.is_empty() {
            self.pipelines.insert(provider_id, next);
        }

        found
    }

    fn observe_failed(&mut self, pipelines: &[Pipeline]) -> Vec<LocalNotification> {
        let mut found = Vec::new();
        for pipeline in pipelines
            .iter()
            .filter(|pipeline| pipeline.status == PipelineStatus::Failed)
        {
            let marks = self.pipelines.entry(pipeline.provider_id).or_default();
            let old = marks.get(&pipeline.id);
            let handled = old.is_some_and(|old| {
                old.status == PipelineStatus::Failed && old.last_run == pipeline.last_run
            });
            let mut mark = PipelineMark::of(pipeline);
            if mark.overdue.is_none() {
                mark.overdue = old.and_then(|old| old.overdue.clone());
            }
            mark.checked_at = old.and_then(|old| old.checked_at);
            marks.insert(pipeline.id.clone(), mark);
            if !handled {
                found.push(self.pipeline_notice(pipeline, format!("{} failed", pipeline.name)));
            }
        }

        found
    }

    fn pipeline_notice(&self, pipeline: &Pipeline, title: String) -> LocalNotification {
        let provider = self
            .providers
            .get(&pipeline.provider_id)
            .map(|mark| mark.name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&pipeline.provider_type);
        let body = [pipeline.repository.as_str(), provider]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");

        LocalNotification {
            key: format!("pipeline:{}", pipeline.id),
            title,
            body,
            pipeline_id: Some(pipeline.id.clone()),
            provider_id: Some(pipeline.provider_id),
        }
    }

    fn deliver(
        &mut self, key: &str, title: &str, body: &str, provider_ids: &[i64], recovered: bool,
    ) -> Option<LocalNotification> {
        let memo = self.delivered.entry(key.to_owned()).or_default();
        memo.providers = provider_ids.to_vec();
        let duplicate = if recovered {
            memo.recovered
        } else {
            memo.titles.iter().any(|known| known == title)
        };
        if duplicate {
            return None;
        }
        if recovered {
            memo.titles.clear();
        } else {
            if memo.titles.len() == TITLES_LIMIT {
                memo.titles.remove(0);
            }
            memo.titles.push(title.to_owned());
        }
        memo.recovered = recovered;

        Some(LocalNotification {
            key: key.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            pipeline_id: None,
            provider_id: provider_ids.first().copied(),
        })
    }

    fn deliver_all(&mut self, alerts: Vec<Alert>) -> Vec<LocalNotification> {
        alerts
            .into_iter()
            .filter_map(|alert| {
                self.deliver(
                    &alert.notification_key,
                    &alert.title,
                    &alert.body,
                    &alert.provider_ids,
                    alert.recovered,
                )
            })
            .collect()
    }
}
