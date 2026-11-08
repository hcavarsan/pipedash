use std::collections::{
    HashMap,
    HashSet,
};
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::time::Instant;

use crate::domain::HealthLevel;
use crate::event::{
    AlertLink,
    CoreEvent,
};

pub(super) const ALERT_COOLDOWN: Duration = Duration::from_secs(15 * 60);
pub(super) const CALM_CHECKS: u8 = 2;
pub(super) const CALM_FETCHES: u8 = 1;

static NEXT_ALERT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Draft {
    pub level: HealthLevel,
    pub title: String,
    pub body: String,
    pub link: Option<AlertLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Candidate {
    pub key: String,
    pub draft: Draft,
    pub provider_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Observation {
    pub candidates: Vec<Candidate>,
    pub recovery: Draft,
    pub provider_ids: Vec<i64>,
    pub calm_needed: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Alert {
    pub alert_id: String,
    pub alert_key: String,
    pub notification_key: String,
    pub provider_ids: Vec<i64>,
    pub level: HealthLevel,
    pub title: String,
    pub body: String,
    pub link: Option<AlertLink>,
    pub recovered: bool,
}

impl Alert {
    fn new(
        subject: &str, key: String, draft: Draft, provider_ids: Vec<i64>, recovered: bool,
    ) -> Self {
        Self {
            alert_id: format!("alert-{}", NEXT_ALERT.fetch_add(1, Ordering::Relaxed)),
            alert_key: key,
            notification_key: subject.to_owned(),
            provider_ids,
            level: draft.level,
            title: draft.title,
            body: draft.body,
            link: draft.link,
            recovered,
        }
    }

    pub(super) fn into_event(self, provider_ids: Vec<i64>) -> CoreEvent {
        CoreEvent::ProviderHealthAlert {
            alert_id: self.alert_id,
            alert_key: self.alert_key,
            notification_key: self.notification_key,
            provider_ids,
            level: self.level,
            title: self.title,
            body: self.body,
            link: self.link,
            recovered: self.recovered,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum KeyState {
    Silent,
    Pending,
    Alerted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Track {
    keys: HashMap<String, KeyState>,
    recoverable: bool,
    streak: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(from = "StoredTracker", into = "StoredTracker")]
pub(super) struct AlertTracker {
    tracks: HashMap<String, Track>,
    last_alert: HashMap<(String, String), Instant>,
}

#[derive(Serialize, Deserialize)]
struct StoredAlert {
    subject: String,
    key: String,
    at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
struct StoredTracker {
    tracks: HashMap<String, Track>,
    #[serde(default)]
    alerted: Vec<StoredAlert>,
}

impl From<AlertTracker> for StoredTracker {
    fn from(tracker: AlertTracker) -> Self {
        let now = Instant::now();
        let wall = Utc::now();
        let alerted = tracker
            .last_alert
            .into_iter()
            .filter_map(|((subject, key), at)| {
                let age = now.saturating_duration_since(at);
                let elapsed = chrono::Duration::from_std(age).ok()?;
                (age < ALERT_COOLDOWN).then(|| StoredAlert {
                    subject,
                    key,
                    at: wall - elapsed,
                })
            })
            .collect();

        Self {
            tracks: tracker.tracks,
            alerted,
        }
    }
}

impl From<StoredTracker> for AlertTracker {
    fn from(stored: StoredTracker) -> Self {
        let now = Instant::now();
        let wall = Utc::now();
        let last_alert = stored
            .alerted
            .into_iter()
            .filter_map(|alert| {
                let age = (wall - alert.at).to_std().unwrap_or_default();
                let at = now.checked_sub(age).filter(|_| age < ALERT_COOLDOWN)?;
                Some(((alert.subject, alert.key), at))
            })
            .collect();

        Self {
            tracks: stored.tracks,
            last_alert,
        }
    }
}

impl AlertTracker {
    pub(super) fn reset(&mut self, subject: &str) {
        self.tracks.insert(subject.to_owned(), Track::default());
    }

    pub(super) fn close(&mut self, subject: &str) {
        self.tracks.remove(subject);
    }

    pub(super) fn has(&self, subject: &str) -> bool {
        self.tracks.contains_key(subject)
    }

    pub(super) fn baseline(&mut self, subject: &str, keys: Vec<String>) {
        let keys = keys
            .into_iter()
            .map(|key| (key, KeyState::Silent))
            .collect();
        self.tracks.insert(
            subject.to_owned(),
            Track {
                keys,
                ..Track::default()
            },
        );
    }

    pub(super) fn arm(&mut self, subject: &str) {
        let track = self.tracks.entry(subject.to_owned()).or_default();
        track.recoverable = true;
        track.streak = 0;
    }

    pub(super) fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.tracks.retain(|subject, _| keep(subject));
        self.last_alert.retain(|(subject, _), _| keep(subject));
    }

    pub(super) fn gap(&mut self, subject: &str) {
        if let Some(track) = self.tracks.get_mut(subject) {
            track.streak = 0;
        }
    }

    pub(super) fn observe(
        &mut self, subject: &str, now: Instant, observation: Observation,
    ) -> Vec<Alert> {
        self.last_alert
            .retain(|_, at| now.saturating_duration_since(*at) < ALERT_COOLDOWN);
        let Some(track) = self.tracks.get_mut(subject) else {
            let keys = observation
                .candidates
                .iter()
                .map(|candidate| (candidate.key.clone(), KeyState::Silent))
                .collect();
            self.tracks.insert(
                subject.to_owned(),
                Track {
                    keys,
                    ..Track::default()
                },
            );
            return Vec::new();
        };

        let present: HashSet<String> = observation
            .candidates
            .iter()
            .map(|candidate| candidate.key.clone())
            .collect();
        track.keys.retain(|key, _| present.contains(key));

        let healthy = observation.candidates.is_empty();
        let mut alerts = Vec::new();
        for candidate in observation.candidates {
            let state = track
                .keys
                .entry(candidate.key.clone())
                .or_insert(KeyState::Pending);
            let cooldown_key = (subject.to_owned(), candidate.key.clone());
            if *state != KeyState::Pending || self.last_alert.contains_key(&cooldown_key) {
                continue;
            }
            *state = KeyState::Alerted;
            track.recoverable = true;
            self.last_alert.insert(cooldown_key, now);
            alerts.push(Alert::new(
                subject,
                candidate.key,
                candidate.draft,
                candidate.provider_ids,
                false,
            ));
        }

        if !healthy {
            track.streak = 0;
        } else if track.recoverable {
            track.streak += 1;
            if track.streak >= observation.calm_needed {
                track.recoverable = false;
                track.streak = 0;
                alerts.push(Alert::new(
                    subject,
                    String::new(),
                    observation.recovery,
                    observation.provider_ids,
                    true,
                ));
            }
        }

        alerts
    }
}

#[cfg(test)]
mod tests;
