use std::collections::HashSet;
use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinError;
use tokio::time::Instant;

use super::alert_candidates::{
    connection_recovery,
    connectivity,
    Connectivity,
};
use super::alerts::{
    Observation,
    CALM_FETCHES,
};
use super::state::{
    default_state,
    ConnectionOutcome,
    FeedState,
    ProviderEntry,
    TaskResult,
};
use super::{
    connection_subject,
    feed_key,
    feed_subject,
    instance_subject,
    Actor,
    FeedKey,
    HealthInput,
    ProviderHealthInput,
    ACTIVATION_MAX_AGE,
};
use crate::application::fetch_scheduler::{
    Generation,
    ScheduleView,
};
use crate::domain::FailureKind;
use crate::infrastructure::ConfigKey;

impl Actor {
    pub(super) async fn handle(&mut self, input: HealthInput) {
        let now = Instant::now();
        match input {
            HealthInput::FetchOutcome {
                provider_id,
                generation,
                view,
                kind,
            } => self.on_fetch_outcome(provider_id, generation, &view, kind, now),
            HealthInput::ProvidersChanged { changed } => self.on_providers_changed(changed, now),
            HealthInput::ForceFeeds { min_age } => self.force_feeds(min_age, now),
            HealthInput::SettingsChanged => self.on_settings(now).await,
        }
        self.tick().await;
    }

    pub(super) async fn finish(&mut self, joined: Result<TaskResult, JoinError>) {
        let now = Instant::now();
        match joined {
            Ok(TaskResult::Feed {
                key,
                token,
                outcome,
            }) => self.on_feed(&key, token, outcome, now),
            Ok(TaskResult::Instance {
                id,
                generation,
                token,
                outcome,
            }) => self.on_instance(id, generation, token, outcome, now),
            Err(e) => tracing::error!(error = %e, "Provider health task failed"),
        }
        self.tick().await;
    }

    pub(super) async fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            let now = Instant::now();
            for feed in self.feeds.values_mut() {
                if feed.in_flight.is_none()
                    && feed.failures == 0
                    && feed.observed.is_none_or(|observed| {
                        now.saturating_duration_since(observed.at) > ACTIVATION_MAX_AGE
                    })
                {
                    feed.force = true;
                }
            }
        }
        self.tick().await;
    }

    fn on_providers_changed(&mut self, changed: Vec<ProviderHealthInput>, now: Instant) {
        for input in changed {
            let ProviderHealthInput {
                id,
                generation,
                display_name,
                source,
                has_instance_checks,
            } = input;
            let Some(generation) = generation else {
                self.providers.remove(&id);
                self.published.remove(&id);
                self.alerts.close(&instance_subject(id));
                self.alerts.close(&connection_subject(id));
                continue;
            };
            match self.providers.get_mut(&id) {
                Some(entry) if entry.generation == generation => {
                    entry.display_name = display_name;
                    entry.source = source;
                    entry.has_instance_checks = has_instance_checks;
                }
                _ => {
                    self.alerts.close(&instance_subject(id));
                    self.alerts.reset(&connection_subject(id));
                    self.published.insert(id, default_state(generation));
                    self.providers.insert(
                        id,
                        ProviderEntry::new(
                            generation,
                            display_name,
                            source,
                            has_instance_checks,
                            now,
                        ),
                    );
                }
            }
        }

        let wanted: HashSet<FeedKey> = self
            .providers
            .values()
            .filter_map(|entry| entry.source.as_ref().map(feed_key))
            .collect();
        self.feeds.retain(|key, _| {
            let keep = wanted.contains(key);
            if !keep {
                self.alerts.close(&feed_subject(key));
            }
            keep
        });
        for key in wanted {
            self.feeds.entry(key).or_insert_with(|| FeedState::new(now));
        }
    }

    fn force_feeds(&mut self, min_age: Duration, now: Instant) {
        for feed in self.feeds.values_mut() {
            if feed.in_flight.is_none()
                && feed
                    .last_attempt
                    .is_none_or(|at| now.saturating_duration_since(at) >= min_age)
            {
                feed.force = true;
            }
        }
    }

    async fn on_settings(&mut self, now: Instant) {
        let enabled = self.config.get_bool(ConfigKey::StatusPagesEnabled).await;
        if enabled == self.enabled {
            return;
        }
        self.enabled = enabled;
        for (key, feed) in &mut self.feeds {
            self.alerts.close(&feed_subject(key));
            *feed = FeedState::new(now);
        }
        for entry in self.providers.values_mut() {
            entry.feed_level = None;
        }
        tracing::info!(enabled, "Provider status pages setting changed");
    }

    fn on_fetch_outcome(
        &mut self, id: i64, generation: Generation, view: &ScheduleView, kind: Option<FailureKind>,
        now: Instant,
    ) {
        let Some(entry) = self
            .providers
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)
        else {
            return;
        };
        entry.last_fetch_ok = kind.is_none();
        entry.outcome = Some(ConnectionOutcome {
            kind,
            failures: view.failures,
            at: Utc::now(),
        });

        let subject = connection_subject(id);
        let candidates = match connectivity(&entry.display_name, kind, view.failures, id) {
            Connectivity::Reset => {
                self.alerts.reset(&subject);
                return;
            }
            Connectivity::Ignore => return,
            Connectivity::Observe(candidates) => candidates,
        };
        let alerts = self.alerts.observe(
            &subject,
            now,
            Observation {
                candidates,
                recovery: connection_recovery(&entry.display_name),
                provider_ids: vec![id],
                calm_needed: CALM_FETCHES,
            },
        );
        self.outbox.extend(alerts);
    }
}
