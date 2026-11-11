use std::collections::HashMap;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::StatusSource;
use reqwest::Client;
use tokio::time::{
    timeout,
    Instant,
};

use super::super::{
    LocalNotification,
    NotificationState,
};
use super::{
    Done,
    Polled,
};
use crate::application::services::health_service::alert_candidates::{
    connection_recovery,
    connectivity,
    feed_candidates,
    feed_observation,
    feed_recovery,
    instance_candidates,
    instance_recovery,
    Connectivity,
};
use crate::application::services::health_service::alerts::{
    Observation,
    CALM_CHECKS,
    CALM_FETCHES,
};
use crate::application::services::health_service::feed::{
    fetch_feed,
    FeedOutcome,
};
use crate::application::services::health_service::state::CheckOutcome;
use crate::application::services::health_service::{
    connection_subject,
    feed_subject,
    instance_subject,
    FeedKey,
};
use crate::application::services::pipeline_service::{
    FETCH_TIMEOUT,
    FETCH_TIMEOUT_MESSAGE,
};
use crate::application::services::provider_service::quota::quota_pause;
use crate::domain::{
    DomainError,
    FailureKind,
    Provider,
};
use crate::infrastructure::status_feeds::{
    evaluate,
    ParsedFeed,
};

const RATE_LIMIT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PAUSE: Duration = Duration::from_secs(60 * 60);

pub(super) async fn read_feed(client: Option<&Client>, key: &FeedKey) -> Option<Box<ParsedFeed>> {
    match fetch_feed(client?, &key.0, key.1, None).await {
        FeedOutcome::Parsed { feed, .. } => Some(feed),
        _ => None,
    }
}

pub(super) async fn poll(provider: &dyn Provider) -> Polled {
    let paused = match timeout(RATE_LIMIT_TIMEOUT, provider.fetch_rate_limit()).await {
        Ok(Ok(Some(rate_limit))) => quota_pause(&rate_limit, Utc::now()),
        _ => None,
    };
    if let Some(until) = paused {
        return Polled::Paused(until);
    }
    match timeout(FETCH_TIMEOUT, provider.fetch_pipelines()).await {
        Ok(Ok(pipelines)) => Polled::Fetched(pipelines),
        Ok(Err(error)) => Polled::Failed(error),
        Err(_) => Polled::Failed(DomainError::Timeout(FETCH_TIMEOUT_MESSAGE.to_string())),
    }
}

impl NotificationState {
    pub(super) fn apply(
        &mut self, done: Done, feeds: &HashMap<FeedKey, Vec<(i64, StatusSource)>>, now: Instant,
        wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        match done {
            Done::Poll { target, polled } => self.settle(target.id, polled, now, wall),
            Done::Health { id, outcome } => {
                let name = self.provider_name(id);
                self.observe_instance(id, &name, Some(outcome), now)
            }
            Done::Overdue { id, checked } => self.settle_overdue(id, checked, wall),
            Done::Feed { key, feed } => match (feed, feeds.get(&key)) {
                (Some(feed), Some(users)) => self.observe_feed(&key, &feed, users, now, wall),
                _ => {
                    self.alerts.gap(&feed_subject(&key));
                    Vec::new()
                }
            },
        }
    }

    fn provider_name(&self, id: i64) -> String {
        self.providers
            .get(&id)
            .map(|mark| mark.name.clone())
            .unwrap_or_default()
    }

    pub(super) fn settle(
        &mut self, id: i64, polled: Polled, now: Instant, wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        let name = self.provider_name(id);
        self.pause(id, None);
        match polled {
            Polled::Paused(until) => {
                self.pause(id, Some(until));
                Vec::new()
            }
            Polled::Failed(error) => {
                let until = error
                    .retry_after()
                    .and_then(|wait| chrono::Duration::from_std(wait.min(MAX_PAUSE)).ok())
                    .map(|wait| wall + wait);
                self.pause(id, until);
                self.observe_connection(id, &name, Some(error.failure_kind()), now)
            }
            Polled::Fetched(pipelines) => {
                let mut found = self.observe_connection(id, &name, None, now);
                found.extend(self.observe_pipelines(id, &pipelines));
                found
            }
        }
    }

    fn observe_connection(
        &mut self, id: i64, name: &str, kind: Option<FailureKind>, now: Instant,
    ) -> Vec<LocalNotification> {
        let mark = self.providers.entry(id).or_default();
        match kind {
            None => mark.failures = 0,
            Some(kind) if kind.is_transient() => {
                mark.failures = mark.failures.saturating_add(1);
            }
            Some(_) => {}
        }
        let subject = connection_subject(id);
        match connectivity(name, kind, mark.failures, id) {
            Connectivity::Reset => {
                self.alerts.reset(&subject);
                Vec::new()
            }
            Connectivity::Ignore => Vec::new(),
            Connectivity::Observe(candidates) => {
                let alerts = self.alerts.observe(
                    &subject,
                    now,
                    Observation {
                        candidates,
                        recovery: connection_recovery(name),
                        provider_ids: vec![id],
                        calm_needed: CALM_FETCHES,
                    },
                );
                self.deliver_all(alerts)
            }
        }
    }

    fn observe_instance(
        &mut self, id: i64, name: &str, health: Option<CheckOutcome>, now: Instant,
    ) -> Vec<LocalNotification> {
        let subject = instance_subject(id);
        match health {
            None => Vec::new(),
            Some(CheckOutcome::Components(components)) => {
                let alerts = self.alerts.observe(
                    &subject,
                    now,
                    Observation {
                        candidates: instance_candidates(name, &components, id),
                        recovery: instance_recovery(name),
                        provider_ids: vec![id],
                        calm_needed: CALM_CHECKS,
                    },
                );
                self.deliver_all(alerts)
            }
            Some(_) => {
                self.alerts.gap(&subject);
                Vec::new()
            }
        }
    }

    fn observe_feed(
        &mut self, key: &FeedKey, feed: &ParsedFeed, users: &[(i64, StatusSource)], now: Instant,
        wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        let mut candidates = Vec::new();
        let mut recovery = None;
        for (id, source) in users {
            let eval = evaluate(feed, &source.components, &source.page_url, wall);
            candidates.extend(feed_candidates(source, &eval, *id));
            recovery.get_or_insert_with(|| feed_recovery(source));
        }
        let provider_ids = users.iter().map(|(id, _)| *id).collect();
        let Some(observation) = feed_observation(candidates, recovery, provider_ids) else {
            return Vec::new();
        };
        let subject = feed_subject(key);
        let seeds: Vec<Option<Vec<String>>> = users
            .iter()
            .map(|(id, _)| {
                self.providers
                    .get_mut(id)
                    .and_then(|mark| mark.feed_seed.take())
            })
            .collect();
        if !self.alerts.has(&subject) && seeds.iter().any(Option::is_some) {
            self.alerts
                .baseline(&subject, seeds.into_iter().flatten().flatten().collect());
        }
        let alerts = self.alerts.observe(&subject, now, observation);

        self.deliver_all(alerts)
    }
}
