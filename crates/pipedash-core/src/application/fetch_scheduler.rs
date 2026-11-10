use std::collections::HashMap;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use entry::after;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::domain::{
    DomainError,
    FailureKind,
};

const MAX_INTERVAL: Duration = Duration::from_secs(365 * 24 * 60 * 60);

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Generation(u64);

impl Generation {
    pub fn next() -> Self {
        Self(NEXT_GENERATION.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    Scheduled,
    Forced,
}

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub max_backoff: Duration,
    pub max_rate_limit: Duration,
    pub outage_floor: Duration,
    pub background_floor: Duration,
    pub forced_min_gap: Duration,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            max_backoff: Duration::from_secs(10 * 60),
            max_rate_limit: Duration::from_secs(60 * 60),
            outage_floor: Duration::from_secs(2 * 60),
            background_floor: Duration::from_secs(5 * 60),
            forced_min_gap: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failed {
        kind: FailureKind,
        retry_after: Option<Duration>,
    },
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum FetchResult {
    Committed(Result<usize, DomainError>),
    Superseded,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleView {
    pub failures: u32,
    pub last_failure: Option<FailureKind>,
    pub next_attempt_in: Duration,
    pub in_flight: bool,
}

type AttemptSender = watch::Sender<Option<FetchResult>>;
type AttemptReceiver = watch::Receiver<Option<FetchResult>>;

struct QuotaPause {
    until: DateTime<Utc>,
    resume: Instant,
}

struct Entry {
    generation: Generation,
    interval: Duration,
    next_attempt: Instant,
    last_attempt: Option<Instant>,
    last_outcome: Option<Outcome>,
    failures: u32,
    last_failure: Option<FailureKind>,
    attempt: Option<AttemptReceiver>,
    quota: Option<QuotaPause>,
    forced_waiters: usize,
}

pub struct FetchScheduler {
    entries: Mutex<HashMap<i64, Entry>>,
    policy: BackoffPolicy,
}

impl Default for FetchScheduler {
    fn default() -> Self {
        Self::new(BackoffPolicy::default())
    }
}

impl FetchScheduler {
    pub fn new(policy: BackoffPolicy) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            policy,
        }
    }

    pub fn policy(&self) -> &BackoffPolicy {
        &self.policy
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<i64, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn register(&self, id: i64, generation: Generation, interval: Duration) {
        let mut entries = self.entries();
        if entries
            .get(&id)
            .is_some_and(|entry| entry.generation == generation)
        {
            return;
        }
        entries.insert(id, Entry::new(generation, interval, Instant::now()));
    }

    pub fn set_interval(&self, id: i64, generation: Generation, interval: Duration) {
        let mut entries = self.entries();
        let Some(entry) = entries
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)
        else {
            return;
        };
        entry.interval = interval.min(MAX_INTERVAL);
        if entry.failures == 0 && entry.attempt.is_none() {
            if let Some(last) = entry.last_attempt {
                entry.next_attempt = after(last, entry.interval);
            }
        }
    }

    pub fn reset(&self, id: i64, generation: Generation, interval: Duration) {
        self.entries()
            .insert(id, Entry::new(generation, interval, Instant::now()));
    }

    pub fn remove(&self, id: i64) {
        self.entries().remove(&id);
    }

    pub fn ids(&self) -> Vec<i64> {
        self.entries().keys().copied().collect()
    }

    pub fn claim_due(
        self: &Arc<Self>, now: Instant, floors: &HashMap<i64, Duration>,
    ) -> Vec<FetchClaim> {
        let mut entries = self.entries();
        let mut claims: Vec<FetchClaim> = entries
            .iter_mut()
            .filter(|(id, entry)| {
                !entry.interval.is_zero()
                    && entry.forced_waiters == 0
                    && entry.attempt.is_none()
                    && now >= entry.due_at(floors.get(*id).copied())
            })
            .map(|(id, entry)| {
                let (result, previous_attempt) = entry.start_attempt(now);
                FetchClaim {
                    scheduler: Arc::clone(self),
                    id: *id,
                    generation: entry.generation,
                    result,
                    previous_attempt,
                    done: false,
                }
            })
            .collect();
        claims.sort_by_key(FetchClaim::id);

        claims
    }

    pub(crate) fn prioritize_forced(
        self: &Arc<Self>, id: i64, generation: Generation,
    ) -> Option<ForcedWait> {
        let mut entries = self.entries();
        let entry = entries
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)?;
        entry.forced_waiters += 1;
        Some(ForcedWait {
            scheduler: Arc::clone(self),
            id,
            generation,
        })
    }

    pub fn claim_forced(
        self: &Arc<Self>, id: i64, generation: Generation, now: Instant,
    ) -> ForcedClaim {
        let mut entries = self.entries();
        let Some(entry) = entries
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)
        else {
            return ForcedClaim::Refused;
        };
        if let Some(attempt) = &entry.attempt {
            return ForcedClaim::Joined(AttemptHandle(attempt.clone()));
        }
        if let Some(resume) = entry
            .quota
            .as_ref()
            .map(|quota| quota.resume)
            .filter(|resume| *resume > now)
        {
            return ForcedClaim::Recent(Outcome::Failed {
                kind: FailureKind::RateLimited,
                retry_after: Some(resume.saturating_duration_since(now)),
            });
        }
        if !entry.forced_delay(&self.policy, now).is_zero() {
            return ForcedClaim::Recent(entry.last_outcome.clone().unwrap_or(Outcome::Cancelled));
        }

        let (result, previous_attempt) = entry.start_attempt(now);
        ForcedClaim::Claimed(FetchClaim {
            scheduler: Arc::clone(self),
            id,
            generation,
            result,
            previous_attempt,
            done: false,
        })
    }

    pub(crate) fn forced_retry_in(
        &self, id: i64, generation: Generation, now: Instant,
    ) -> Option<Duration> {
        self.entries()
            .get(&id)
            .filter(|entry| entry.generation == generation)
            .map(|entry| entry.forced_delay(&self.policy, now))
    }

    pub fn view(&self, id: i64, now: Instant) -> Option<ScheduleView> {
        self.entries().get(&id).map(|entry| entry.view(now))
    }

    pub fn next_due_in(&self, id: i64, now: Instant, floor: Option<Duration>) -> Option<Duration> {
        self.entries()
            .get(&id)
            .map(|entry| entry.due_at(floor).saturating_duration_since(now))
    }

    pub fn set_quota_pause(
        &self, id: i64, generation: Generation, until: Option<DateTime<Utc>>, now: Instant,
        wall: DateTime<Utc>,
    ) -> bool {
        let mut entries = self.entries();
        let Some(entry) = entries
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)
        else {
            return false;
        };
        let previous = entry.quota.as_ref().map(|quota| quota.until);
        let pause = until.and_then(|until| {
            (until - wall)
                .to_std()
                .ok()
                .filter(|wait| !wait.is_zero())
                .map(|wait| QuotaPause {
                    until,
                    resume: after(now, wait),
                })
        });
        if let Some(pause) = &pause {
            entry.next_attempt = entry.next_attempt.max(pause.resume);
        }
        let current = pause.as_ref().map(|pause| pause.until);
        entry.quota = pause;

        previous != current
    }

    pub fn paused_until(&self, id: i64, wall: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.entries()
            .get(&id)
            .and_then(|entry| entry.quota.as_ref())
            .map(|quota| quota.until)
            .filter(|until| *until > wall)
    }

    fn finish(
        &self, id: i64, generation: Generation, now: Instant, outcome: Outcome,
    ) -> Option<ScheduleView> {
        let mut entries = self.entries();
        let entry = entries
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)?;
        entry.finish(&self.policy, now, outcome);

        Some(entry.view(now))
    }

    fn release(&self, id: i64, generation: Generation, previous_attempt: Option<Instant>) {
        if let Some(entry) = self
            .entries()
            .get_mut(&id)
            .filter(|entry| entry.generation == generation)
        {
            entry.release(previous_attempt);
        }
    }
}

#[derive(Debug)]
pub enum ForcedClaim {
    Claimed(FetchClaim),
    Joined(AttemptHandle),
    Recent(Outcome),
    Refused,
}

pub(crate) struct ForcedWait {
    scheduler: Arc<FetchScheduler>,
    id: i64,
    generation: Generation,
}

pub struct FetchClaim {
    scheduler: Arc<FetchScheduler>,
    id: i64,
    generation: Generation,
    result: AttemptSender,
    previous_attempt: Option<Instant>,
    done: bool,
}

pub struct AttemptHandle(AttemptReceiver);

mod claims;
mod entry;
#[cfg(test)]
mod tests;
