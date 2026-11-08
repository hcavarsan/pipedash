use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use super::checks::run_check;
use super::feed::{
    fetch_feed,
    FEED_UNREACHABLE,
};
use super::signals::{
    next_evidence_change,
    Evidence,
};
use super::state::{
    feed_evidence,
    FeedState,
    TaskResult,
};
use super::{
    feed_key,
    feed_subject,
    instance_subject,
    Actor,
    FeedKey,
    FEED_INTERVAL,
    FEED_INTERVAL_FAILING,
    JOB_CONCURRENCY,
};

impl Actor {
    fn feed_interval(&self, key: &FeedKey) -> Duration {
        let failing = self.providers.values().any(|entry| {
            entry.failing_transiently()
                && entry.source.as_ref().is_some_and(|s| feed_key(s) == *key)
        });

        if failing {
            FEED_INTERVAL_FAILING
        } else {
            FEED_INTERVAL
        }
    }

    fn feed_due_at(&self, key: &FeedKey, feed: &FeedState, now: Instant) -> Instant {
        if feed.force {
            return now;
        }
        match feed.observed {
            Some(observed) if feed.failures == 0 => observed.at + self.feed_interval(key),
            _ => feed.next_try,
        }
    }

    fn spawn_due(&mut self, now: Instant) {
        if self.enabled && self.active {
            let due: Vec<FeedKey> = self
                .feeds
                .iter()
                .filter(|(key, feed)| {
                    feed.in_flight.is_none() && self.feed_due_at(key, feed, now) <= now
                })
                .map(|(key, _)| key.clone())
                .collect();
            for key in due {
                if self.tasks.len() >= JOB_CONCURRENCY {
                    return;
                }
                self.spawn_feed(key, now);
            }
        }
        if self.active {
            let due: Vec<i64> = self
                .providers
                .iter()
                .filter(|(_, entry)| entry.instance_due(now))
                .map(|(id, _)| *id)
                .collect();
            for id in due {
                if self.tasks.len() >= JOB_CONCURRENCY {
                    return;
                }
                self.spawn_instance(id);
            }
        }
    }

    fn spawn_feed(&mut self, key: FeedKey, now: Instant) {
        let Some(feed) = self.feeds.get_mut(&key) else {
            return;
        };
        let client = match self.http.status_client() {
            Ok(client) => client,
            Err(e) => {
                tracing::error!(error = %e, "Failed to build the status page HTTP client");
                feed.failed(now, FEED_UNREACHABLE, None);
                feed.force = false;
                return;
            }
        };
        self.next_token += 1;
        let token = self.next_token;
        feed.in_flight = Some(token);
        feed.force = false;
        feed.last_attempt = Some(now);
        let etag = feed.etag.clone();
        tracing::debug!(feed = %key.0, "Requesting status feed");

        self.tasks.spawn(async move {
            let outcome = fetch_feed(&client, &key.0, key.1, etag.as_deref()).await;

            TaskResult::Feed {
                key,
                token,
                outcome,
            }
        });
    }

    fn spawn_instance(&mut self, id: i64) {
        let Some(entry) = self.providers.get_mut(&id) else {
            return;
        };
        self.next_token += 1;
        let token = self.next_token;
        entry.instance.in_flight = Some(token);
        let generation = entry.generation;
        let service = Arc::clone(&self.provider_service);

        self.tasks.spawn(async move {
            let outcome = run_check(service, id, generation).await;

            TaskResult::Instance {
                id,
                generation,
                token,
                outcome,
            }
        });
    }

    pub(super) fn next_wake(&self, now: Instant) -> Option<Instant> {
        let mut wake: Option<Instant> = None;
        let mut note = |at: Instant| wake = Some(wake.map_or(at, |known| known.min(at)));

        let slot = self.tasks.len() < JOB_CONCURRENCY;
        if slot && self.enabled && self.active {
            for (key, feed) in &self.feeds {
                if feed.in_flight.is_none() {
                    note(self.feed_due_at(key, feed, now));
                }
            }
        }
        for feed in self.feeds.values() {
            if let Some(at) = next_evidence_change(feed.observed, now) {
                note(at);
            }
        }
        for entry in self.providers.values() {
            if slot
                && self.active
                && entry.has_instance_checks
                && entry.last_fetch_ok
                && entry.instance.in_flight.is_none()
            {
                note(entry.instance.next_check);
            }
            if let Some(at) = next_evidence_change(entry.instance.observed, now) {
                note(at);
            }
        }

        wake
    }

    pub(super) fn detect_gaps(&mut self, now: Instant) {
        for (key, feed) in &mut self.feeds {
            if !feed.fresh || feed_evidence(feed, now).is_some_and(Evidence::is_fresh) {
                continue;
            }
            feed.fresh = false;
            self.alerts.gap(&feed_subject(key));
            for entry in self.providers.values_mut() {
                if entry.source.as_ref().is_some_and(|s| feed_key(s) == *key) {
                    entry.feed_level = None;
                }
            }
        }
        for (id, entry) in &mut self.providers {
            let instance = &mut entry.instance;
            let fresh = Evidence::of(instance.observed, instance.unavailable, now)
                .is_some_and(Evidence::is_fresh);
            if instance.fresh && !fresh {
                instance.fresh = false;
                self.alerts.gap(&instance_subject(*id));
            }
        }
    }

    pub(super) async fn tick(&mut self) {
        let now = Instant::now();
        self.spawn_due(now);
        self.refresh(now).await;
    }
}
