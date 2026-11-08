use std::collections::HashMap;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    ComponentHealth,
    StatusSource,
};
use tokio::time::Instant;

use super::feed::FeedOutcome;
use super::signals::{
    Evidence,
    Observed,
    UnavailableReason,
};
use super::{
    FeedKey,
    ProviderHealthState,
    FEED_BACKOFF_BASE,
    FEED_BACKOFF_MAX,
};
use crate::application::fetch_scheduler::Generation;
use crate::domain::{
    FailureKind,
    HealthLevel,
    SignalKind,
};
use crate::infrastructure::status_feeds::ParsedFeed;

pub(super) struct FeedState {
    pub(super) etag: Option<String>,
    pub(super) last_good: Option<ParsedFeed>,
    pub(super) observed: Option<Observed>,
    pub(super) failures: u32,
    pub(super) next_try: Instant,
    pub(super) last_attempt: Option<Instant>,
    pub(super) in_flight: Option<u64>,
    pub(super) error: Option<&'static str>,
    pub(super) force: bool,
    pub(super) fresh: bool,
    pub(super) logged_unknown: bool,
    pub(super) logged_fallback: bool,
}

impl FeedState {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            etag: None,
            last_good: None,
            observed: None,
            failures: 0,
            next_try: now,
            last_attempt: None,
            in_flight: None,
            error: None,
            force: false,
            fresh: false,
            logged_unknown: false,
            logged_fallback: false,
        }
    }

    pub(super) fn failed(
        &mut self, now: Instant, message: &'static str, retry_after: Option<Duration>,
    ) {
        self.failures = self.failures.saturating_add(1);
        self.error = Some(message);
        let backoff = FEED_BACKOFF_BASE
            .saturating_mul(1u32 << (self.failures - 1).min(10))
            .min(FEED_BACKOFF_MAX);
        self.next_try = now + backoff.max(retry_after.unwrap_or_default());
    }
}

pub(super) fn feed_evidence(feed: &FeedState, now: Instant) -> Option<Evidence> {
    Evidence::of(feed.observed, None, now).or_else(|| {
        feed.error
            .map(|_| Evidence::Unavailable(UnavailableReason::FeedError))
    })
}

pub(super) struct InstanceState {
    pub(super) components: Vec<ComponentHealth>,
    pub(super) observed: Option<Observed>,
    pub(super) unavailable: Option<UnavailableReason>,
    pub(super) in_flight: Option<u64>,
    pub(super) next_check: Instant,
    pub(super) fresh: bool,
}

impl InstanceState {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            components: Vec::new(),
            observed: None,
            unavailable: None,
            in_flight: None,
            next_check: now,
            fresh: false,
        }
    }
}

pub(super) struct ConnectionOutcome {
    pub(super) kind: Option<FailureKind>,
    pub(super) failures: u32,
    pub(super) at: DateTime<Utc>,
}

pub(super) struct ProviderEntry {
    pub(super) generation: Generation,
    pub(super) display_name: String,
    pub(super) source: Option<StatusSource>,
    pub(super) has_instance_checks: bool,
    pub(super) instance: InstanceState,
    pub(super) outcome: Option<ConnectionOutcome>,
    pub(super) last_fetch_ok: bool,
    pub(super) episodes: HashMap<SignalKind, String>,
    pub(super) feed_level: Option<HealthLevel>,
}

impl ProviderEntry {
    pub(super) fn new(
        generation: Generation, display_name: String, source: Option<StatusSource>,
        has_instance_checks: bool, now: Instant,
    ) -> Self {
        Self {
            generation,
            display_name,
            source,
            has_instance_checks,
            instance: InstanceState::new(now),
            outcome: None,
            last_fetch_ok: false,
            episodes: HashMap::new(),
            feed_level: None,
        }
    }

    pub(super) fn instance_due(&self, now: Instant) -> bool {
        self.has_instance_checks
            && self.last_fetch_ok
            && self.instance.in_flight.is_none()
            && now >= self.instance.next_check
    }

    pub(super) fn failing_transiently(&self) -> bool {
        self.outcome
            .as_ref()
            .and_then(|outcome| outcome.kind)
            .is_some_and(FailureKind::is_transient)
    }
}

pub(super) enum CheckOutcome {
    Components(Vec<ComponentHealth>),
    Denied,
    NotInstalled,
    Failed,
}

pub(super) enum TaskResult {
    Feed {
        key: FeedKey,
        token: u64,
        outcome: FeedOutcome,
    },
    Instance {
        id: i64,
        generation: Generation,
        token: u64,
        outcome: CheckOutcome,
    },
}

pub(super) fn default_state(generation: Generation) -> ProviderHealthState {
    ProviderHealthState {
        generation,
        level: HealthLevel::Operational,
        signals: Vec::new(),
        status_page: None,
        outage_floor: false,
    }
}
