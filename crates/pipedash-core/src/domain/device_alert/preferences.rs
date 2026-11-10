use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Duration,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    AlertKind,
    NotificationPreferences,
    OnceWatch,
};
use crate::domain::{
    DomainError,
    DomainResult,
};

const WATCH_ONCE_TTL_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PreferenceChange {
    Kind { kind: AlertKind, enabled: bool },
    ProviderMuted { provider_id: i64, muted: bool },
    PipelineMuted { pipeline_id: String, muted: bool },
    PipelineWatched { pipeline_id: String, watched: bool },
    WatchNextFinish { pipeline_id: String },
    CancelNextFinish { pipeline_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineNotifyState {
    Muted,
    Watching,
    Pending,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPreferenceSnapshot {
    pub preferences: NotificationPreferences,
    pub pipeline_states: BTreeMap<String, PipelineNotifyState>,
}

impl NotificationPreferenceSnapshot {
    pub fn new(preferences: NotificationPreferences) -> Self {
        let pipeline_states = preferences.pipeline_states();

        Self {
            preferences,
            pipeline_states,
        }
    }
}

fn toggled<T: PartialEq>(list: &mut Vec<T>, item: T, present: bool) {
    list.retain(|existing| *existing != item);
    if present {
        list.push(item);
    }
}

fn without(list: &mut Vec<String>, item: &str) {
    list.retain(|existing| existing != item);
}

fn without_once(watch_once: &mut Vec<OnceWatch>, pipeline_id: &str) {
    watch_once.retain(|watch| watch.pipeline_id != pipeline_id);
}

impl NotificationPreferences {
    pub fn merge(&mut self, incoming: &Self) -> bool {
        let mut next = incoming.clone();
        next.keep_fired(self);
        let changed = *self != next;
        *self = next;

        changed
    }

    pub fn drop_stale_watches(&mut self, now: DateTime<Utc>) -> bool {
        let before = self.watch_once.len();
        let ttl = Duration::days(WATCH_ONCE_TTL_DAYS);
        self.watch_once.retain(|watch| now - watch.since <= ttl);

        self.watch_once.len() != before
    }

    pub fn pipeline_states(&self) -> BTreeMap<String, PipelineNotifyState> {
        let mut states = BTreeMap::new();
        for watch in self
            .watch_once
            .iter()
            .filter(|watch| watch.fired_at.is_none())
        {
            states.insert(watch.pipeline_id.clone(), PipelineNotifyState::Pending);
        }
        for id in &self.watched_pipelines {
            states.insert(id.clone(), PipelineNotifyState::Watching);
        }
        for id in &self.muted_pipelines {
            states.insert(id.clone(), PipelineNotifyState::Muted);
        }

        states
    }
}

pub fn merge_fired_watches(mut known: Vec<OnceWatch>, incoming: Vec<OnceWatch>) -> Vec<OnceWatch> {
    known.extend(incoming);
    known.sort_by(|a, b| (&a.pipeline_id, a.since).cmp(&(&b.pipeline_id, b.since)));
    known.dedup_by(|a, b| a.pipeline_id == b.pipeline_id && a.since == b.since);

    known
}

pub fn change_preferences(
    preferences: &mut NotificationPreferences, change: PreferenceChange, now: DateTime<Utc>,
) -> DomainResult<()> {
    preferences.drop_stale_watches(now);

    match change {
        PreferenceChange::Kind { kind, enabled } => {
            let target = match kind {
                AlertKind::Failed => &mut preferences.failed,
                AlertKind::Degraded => &mut preferences.degraded,
                AlertKind::LongRun => &mut preferences.long_run,
                AlertKind::Outage => &mut preferences.outage,
                AlertKind::Recovered => &mut preferences.recovered,
                AlertKind::Finished | AlertKind::Test => {
                    return Err(DomainError::InvalidConfig(format!(
                        "{} notifications can't be switched on or off",
                        kind.as_str()
                    )));
                }
            };
            *target = enabled;
        }
        PreferenceChange::ProviderMuted { provider_id, muted } => {
            toggled(&mut preferences.muted_providers, provider_id, muted);
        }
        PreferenceChange::PipelineMuted { pipeline_id, muted } => {
            if muted {
                without(&mut preferences.watched_pipelines, &pipeline_id);
                without_once(&mut preferences.watch_once, &pipeline_id);
            }
            toggled(&mut preferences.muted_pipelines, pipeline_id, muted);
        }
        PreferenceChange::PipelineWatched {
            pipeline_id,
            watched,
        } => {
            if watched {
                without(&mut preferences.muted_pipelines, &pipeline_id);
                without_once(&mut preferences.watch_once, &pipeline_id);
            }
            toggled(&mut preferences.watched_pipelines, pipeline_id, watched);
        }
        PreferenceChange::WatchNextFinish { pipeline_id } => {
            without(&mut preferences.muted_pipelines, &pipeline_id);
            without(&mut preferences.watched_pipelines, &pipeline_id);
            without_once(&mut preferences.watch_once, &pipeline_id);
            preferences.watch_once.push(OnceWatch {
                pipeline_id,
                since: now,
                fired_at: None,
            });
        }
        PreferenceChange::CancelNextFinish { pipeline_id } => {
            without_once(&mut preferences.watch_once, &pipeline_id);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
