use chrono::Utc;
use tokio::time::Instant;

use super::alert_candidates::{
    feed_candidates,
    feed_observation,
    feed_recovery,
    instance_candidates,
    instance_recovery,
};
use super::alerts::{
    Observation,
    CALM_CHECKS,
};
use super::feed::{
    FeedOutcome,
    FEED_INVALID,
};
use super::signals::{
    Observed,
    UnavailableReason,
};
use super::state::CheckOutcome;
use super::{
    feed_key,
    feed_subject,
    instance_subject,
    Actor,
    FeedKey,
    INSTANCE_INTERVAL,
};
use crate::application::fetch_scheduler::Generation;
use crate::domain::HealthLevel;
use crate::infrastructure::status_feeds::evaluate;

impl Actor {
    pub(super) fn on_feed(
        &mut self, key: &FeedKey, token: u64, outcome: FeedOutcome, now: Instant,
    ) {
        let Some(feed) = self.feeds.get_mut(key) else {
            return;
        };
        if feed.in_flight != Some(token) {
            return;
        }
        feed.in_flight = None;

        let observed = match outcome {
            FeedOutcome::Parsed { feed: parsed, etag } => {
                if !feed.logged_unknown && !parsed.unknown.is_empty() {
                    feed.logged_unknown = true;
                    tracing::warn!(
                        values = ?parsed.unknown,
                        "Status feed has values Pipedash doesn't know, ignoring them"
                    );
                }
                feed.last_good = Some(*parsed);
                feed.etag = etag;
                true
            }
            FeedOutcome::NotModified if feed.last_good.is_some() => true,
            FeedOutcome::NotModified => {
                feed.etag = None;
                feed.failed(now, FEED_INVALID, None);
                false
            }
            FeedOutcome::Failed {
                message,
                retry_after,
            } => {
                feed.failed(now, message, retry_after);
                false
            }
        };
        if !observed {
            return;
        }

        feed.observed = Some(Observed {
            at: now,
            wall: Utc::now(),
        });
        feed.failures = 0;
        feed.error = None;
        feed.fresh = true;
        feed.next_try = now;
        self.observe_feed(key, now);
    }

    fn observe_feed(&mut self, key: &FeedKey, now: Instant) {
        let wall = Utc::now();
        let Some(feed) = self.feeds.get_mut(key) else {
            return;
        };
        let Some(parsed) = &feed.last_good else {
            return;
        };
        let mut users: Vec<i64> = self
            .providers
            .iter()
            .filter(|(_, entry)| entry.source.as_ref().is_some_and(|s| feed_key(s) == *key))
            .map(|(id, _)| *id)
            .collect();
        users.sort_unstable();

        let mut candidates = Vec::new();
        let mut recovered = Vec::new();
        let mut recovery = None;
        for id in &users {
            let Some(entry) = self.providers.get_mut(id) else {
                continue;
            };
            let Some(source) = &entry.source else {
                continue;
            };
            let eval = evaluate(parsed, &source.components, &source.page_url, wall);
            if eval.filter_fallback && !feed.logged_fallback {
                feed.logged_fallback = true;
                tracing::warn!(
                    source = %source.name,
                    "None of the watched component ids exist in the status feed, watching all components"
                );
            }
            candidates.extend(feed_candidates(source, &eval, *id));
            let previous = entry.feed_level.replace(eval.level);
            if previous == Some(HealthLevel::MajorOutage) && eval.level < HealthLevel::PartialOutage
            {
                recovered.push((*id, entry.generation));
            }
            recovery.get_or_insert_with(|| feed_recovery(source));
        }
        let Some(observation) = feed_observation(candidates, recovery, users) else {
            return;
        };

        let alerts = self.alerts.observe(&feed_subject(key), now, observation);
        self.outbox.extend(alerts);
        self.recoveries.extend(recovered);
    }

    pub(super) fn on_instance(
        &mut self, id: i64, generation: Generation, token: u64, outcome: CheckOutcome, now: Instant,
    ) {
        let Some(entry) = self
            .providers
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)
        else {
            return;
        };
        let instance = &mut entry.instance;
        if instance.in_flight != Some(token) {
            return;
        }
        instance.in_flight = None;
        instance.next_check = now + INSTANCE_INTERVAL;

        match outcome {
            CheckOutcome::Components(components) => {
                instance.components = components;
                instance.observed = Some(Observed {
                    at: now,
                    wall: Utc::now(),
                });
                instance.unavailable = None;
                instance.fresh = true;
                let alerts = self.alerts.observe(
                    &instance_subject(id),
                    now,
                    Observation {
                        candidates: instance_candidates(
                            &entry.display_name,
                            &instance.components,
                            id,
                        ),
                        recovery: instance_recovery(&entry.display_name),
                        provider_ids: vec![id],
                        calm_needed: CALM_CHECKS,
                    },
                );
                self.outbox.extend(alerts);
            }
            CheckOutcome::Denied => {
                instance.unavailable = Some(UnavailableReason::PermissionDenied);
                tracing::debug!(
                    provider_id = id,
                    reason = UnavailableReason::PermissionDenied.label(),
                    "Controller health is unavailable"
                );
            }
            CheckOutcome::NotInstalled => {
                instance.unavailable = Some(UnavailableReason::NotInstalled);
                tracing::debug!(
                    provider_id = id,
                    reason = UnavailableReason::NotInstalled.label(),
                    "Controller health is unavailable"
                );
            }
            CheckOutcome::Failed => {}
        }
    }
}
