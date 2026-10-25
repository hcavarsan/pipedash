use std::collections::HashMap;
use std::fmt;
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
use tokio::sync::watch;
use tokio::time::Instant;

use crate::domain::{
    DomainError,
    FailureKind,
};

const FAR_FUTURE: Duration = Duration::from_secs(10 * 365 * 24 * 60 * 60);
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
    pub forced_min_gap: Duration,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            max_backoff: Duration::from_secs(10 * 60),
            max_rate_limit: Duration::from_secs(60 * 60),
            outage_floor: Duration::from_secs(2 * 60),
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
}

impl Entry {
    fn new(generation: Generation, interval: Duration, now: Instant) -> Self {
        Self {
            generation,
            interval: interval.min(MAX_INTERVAL),
            next_attempt: now,
            last_attempt: None,
            last_outcome: None,
            failures: 0,
            last_failure: None,
            attempt: None,
            quota: None,
        }
    }

    fn view(&self, now: Instant) -> ScheduleView {
        ScheduleView {
            failures: self.failures,
            last_failure: self.last_failure,
            next_attempt_in: self.next_attempt.saturating_duration_since(now),
            in_flight: self.attempt.is_some(),
        }
    }

    fn due_at(&self, floor: Option<Duration>) -> Instant {
        match (floor, self.last_attempt) {
            (Some(floor), Some(last)) => self.next_attempt.max(after(last, floor)),
            _ => self.next_attempt,
        }
    }

    fn forced_delay(&self, policy: &BackoffPolicy, now: Instant) -> Duration {
        let mut resume = self
            .quota
            .as_ref()
            .map_or(now, |quota| quota.resume.max(now));
        if matches!(
            self.last_outcome,
            Some(Outcome::Failed {
                kind: FailureKind::RateLimited,
                ..
            })
        ) {
            resume = resume.max(self.next_attempt);
        }
        if let Some(last) = self.last_attempt {
            resume = resume.max(after(last, policy.forced_min_gap));
        }
        resume.saturating_duration_since(now)
    }

    fn start_attempt(&mut self, now: Instant) -> (AttemptSender, Option<Instant>) {
        let (sender, receiver) = watch::channel(None);
        self.attempt = Some(receiver);
        (sender, self.last_attempt.replace(now))
    }

    fn release(&mut self, previous_attempt: Option<Instant>) {
        self.attempt = None;
        self.last_attempt = previous_attempt;
    }

    fn finish(&mut self, policy: &BackoffPolicy, now: Instant, outcome: Outcome) {
        let interval = self.interval;
        let wait = match &outcome {
            Outcome::Success => {
                self.failures = 0;
                self.last_failure = None;
                interval
            }
            Outcome::Cancelled => interval,
            Outcome::Failed { kind, retry_after } => {
                self.last_failure = Some(*kind);
                if *kind == FailureKind::RateLimited {
                    self.failures = self.failures.saturating_add(1);
                    let wait = (*retry_after).unwrap_or_else(|| doubled(interval, self.failures));
                    interval.max(wait.min(policy.max_rate_limit))
                } else if kind.is_transient() {
                    self.failures = self.failures.saturating_add(1);
                    let hint = (*retry_after)
                        .unwrap_or_default()
                        .min(policy.max_rate_limit);
                    interval
                        .max(doubled(interval, self.failures).min(policy.max_backoff))
                        .max(hint)
                } else {
                    interval
                }
            }
        };
        self.attempt = None;
        self.last_outcome = Some(outcome);
        self.next_attempt = after(now, wait);
        match self.quota.as_ref().map(|quota| quota.resume) {
            Some(resume) if resume > now => self.next_attempt = self.next_attempt.max(resume),
            Some(_) => self.quota = None,
            None => {}
        }
    }
}

fn doubled(interval: Duration, failures: u32) -> Duration {
    interval.saturating_mul(1u32.checked_shl(failures).unwrap_or(u32::MAX))
}

fn after(now: Instant, wait: Duration) -> Instant {
    now.checked_add(wait).unwrap_or_else(|| now + FAR_FUTURE)
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

    pub fn claim_due(
        self: &Arc<Self>, now: Instant, floors: &HashMap<i64, Duration>,
    ) -> Vec<FetchClaim> {
        let mut entries = self.entries();
        let mut claims: Vec<FetchClaim> = entries
            .iter_mut()
            .filter(|(id, entry)| {
                !entry.interval.is_zero()
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

pub struct FetchClaim {
    scheduler: Arc<FetchScheduler>,
    id: i64,
    generation: Generation,
    result: AttemptSender,
    previous_attempt: Option<Instant>,
    done: bool,
}

impl fmt::Debug for FetchClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FetchClaim")
            .field("id", &self.id)
            .field("generation", &self.generation)
            .finish()
    }
}

impl FetchClaim {
    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn handle(&self) -> AttemptHandle {
        AttemptHandle(self.result.subscribe())
    }

    pub fn complete(
        mut self, now: Instant, outcome: Outcome, result: FetchResult,
    ) -> Option<ScheduleView> {
        self.done = true;
        self.result.send_replace(Some(result));

        self.scheduler
            .finish(self.id, self.generation, now, outcome)
    }

    pub fn release(mut self) {
        self.done = true;
        self.result.send_replace(Some(FetchResult::Cancelled));
        self.scheduler
            .release(self.id, self.generation, self.previous_attempt);
    }
}

impl Drop for FetchClaim {
    fn drop(&mut self) {
        if !self.done {
            self.scheduler
                .finish(self.id, self.generation, Instant::now(), Outcome::Cancelled);
        }
    }
}

pub struct AttemptHandle(AttemptReceiver);

impl fmt::Debug for AttemptHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AttemptHandle")
    }
}

impl AttemptHandle {
    pub async fn wait(mut self) -> FetchResult {
        match self.0.wait_for(Option::is_some).await {
            Ok(result) => result.clone().unwrap_or(FetchResult::Cancelled),
            Err(_) => FetchResult::Cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    fn scheduler_with(entries: &[(i64, u64)]) -> (Arc<FetchScheduler>, HashMap<i64, Generation>) {
        let scheduler = Arc::new(FetchScheduler::default());
        let generations = entries
            .iter()
            .map(|(id, interval)| {
                let generation = Generation::next();
                scheduler.register(*id, generation, secs(*interval));
                (*id, generation)
            })
            .collect();

        (scheduler, generations)
    }

    fn no_floors() -> HashMap<i64, Duration> {
        HashMap::new()
    }

    fn due_ids(scheduler: &Arc<FetchScheduler>, now: Instant) -> Vec<i64> {
        scheduler
            .claim_due(now, &no_floors())
            .iter()
            .map(FetchClaim::id)
            .collect()
    }

    fn only_claim(scheduler: &Arc<FetchScheduler>, now: Instant) -> FetchClaim {
        let mut claims = scheduler.claim_due(now, &no_floors());
        assert_eq!(claims.len(), 1);
        claims.remove(0)
    }

    fn failed(kind: FailureKind, retry_after: Option<Duration>) -> Outcome {
        Outcome::Failed { kind, retry_after }
    }

    fn finish_with(claim: FetchClaim, now: Instant, outcome: Outcome) -> ScheduleView {
        let result = match &outcome {
            Outcome::Failed { .. } => {
                FetchResult::Committed(Err(DomainError::NetworkError("unreachable".to_string())))
            }
            _ => FetchResult::Committed(Ok(0)),
        };
        claim.complete(now, outcome, result).unwrap()
    }

    fn due_after(seconds: u64) -> Vec<i64> {
        let (scheduler, generations) = scheduler_with(&[(1, 10), (2, 600), (3, 0), (4, 30)]);
        let start = Instant::now();
        for id in [1, 2] {
            let ForcedClaim::Claimed(claim) = scheduler.claim_forced(id, generations[&id], start)
            else {
                panic!("expected a claim for {id}");
            };
            finish_with(claim, start, Outcome::Success);
        }

        due_ids(&scheduler, start + secs(seconds))
    }

    #[test]
    fn each_provider_waits_for_its_own_interval() {
        assert_eq!(due_after(5), vec![4]);
        assert_eq!(due_after(60), vec![1, 4]);
        assert_eq!(due_after(599), vec![1, 4]);
        assert_eq!(due_after(600), vec![1, 2, 4]);
    }

    #[test]
    fn counters_do_not_change_from_view_or_from_claiming() {
        let (scheduler, _) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();

        let before = scheduler.view(1, now).unwrap();
        assert_eq!(scheduler.view(1, now).unwrap(), before);
        let claim = only_claim(&scheduler, now);
        let claimed = scheduler.view(1, now).unwrap();

        assert_eq!(claimed.failures, before.failures);
        assert_eq!(claimed.next_attempt_in, before.next_attempt_in);
        assert!(claimed.in_flight);
        drop(claim);
    }

    #[test]
    fn transient_failures_double_up_to_the_cap() {
        let (scheduler, _) = scheduler_with(&[(1, 30)]);
        let mut now = Instant::now();
        let mut waits = Vec::new();
        for _ in 0..7 {
            let view = finish_with(
                only_claim(&scheduler, now),
                now,
                failed(FailureKind::Network, None),
            );
            waits.push(view.next_attempt_in.as_secs());
            now += view.next_attempt_in;
        }

        assert_eq!(waits, vec![60, 120, 240, 480, 600, 600, 600]);
        assert_eq!(scheduler.view(1, now).unwrap().failures, 7);
    }

    #[test]
    fn timeouts_and_unavailable_count_like_network_failures() {
        for kind in [FailureKind::Timeout, FailureKind::Unavailable] {
            let (scheduler, _) = scheduler_with(&[(1, 30)]);
            let now = Instant::now();

            let view = finish_with(only_claim(&scheduler, now), now, failed(kind, None));

            assert_eq!(view.failures, 1, "{kind:?}");
            assert_eq!(view.next_attempt_in, secs(60), "{kind:?}");
        }
    }

    #[test]
    fn a_server_hint_extends_the_wait_but_never_shortens_it() {
        let (scheduler, _) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();

        let long = finish_with(
            only_claim(&scheduler, now),
            now,
            failed(FailureKind::Unavailable, Some(secs(300))),
        );
        let later = now + long.next_attempt_in;
        let short = finish_with(
            only_claim(&scheduler, later),
            later,
            failed(FailureKind::Unavailable, Some(secs(5))),
        );

        assert_eq!(long.next_attempt_in, secs(300));
        assert_eq!(short.next_attempt_in, secs(120));
    }

    #[test]
    fn rate_limits_use_retry_after_capped_at_an_hour() {
        let (scheduler, _) = scheduler_with(&[(1, 30)]);
        let mut now = Instant::now();
        let mut waits = Vec::new();
        for retry_after in [Some(secs(120)), Some(secs(2 * 3600)), None, Some(secs(5))] {
            let view = finish_with(
                only_claim(&scheduler, now),
                now,
                failed(FailureKind::RateLimited, retry_after),
            );
            waits.push(view.next_attempt_in.as_secs());
            now += view.next_attempt_in;
        }

        assert_eq!(waits, vec![120, 3600, 30 * 8, 30]);
    }

    #[test]
    fn an_interval_longer_than_the_wait_caps_is_kept() {
        let (scheduler, _) = scheduler_with(&[(1, 2 * 3600), (2, 2 * 3600)]);
        let now = Instant::now();
        let claims = scheduler.claim_due(now, &no_floors());
        let mut views = HashMap::new();
        for claim in claims {
            let id = claim.id();
            let outcome = if id == 1 {
                failed(FailureKind::RateLimited, Some(secs(3600)))
            } else {
                failed(FailureKind::Network, None)
            };
            views.insert(id, finish_with(claim, now, outcome));
        }

        assert_eq!(views[&1].next_attempt_in, secs(2 * 3600));
        assert_eq!(views[&2].next_attempt_in, secs(2 * 3600));
    }

    #[test]
    fn auth_failures_do_not_count_and_success_resets() {
        let (scheduler, _) = scheduler_with(&[(1, 30)]);
        let mut now = Instant::now();

        let network = finish_with(
            only_claim(&scheduler, now),
            now,
            failed(FailureKind::Network, None),
        );
        now += network.next_attempt_in;
        let auth = finish_with(
            only_claim(&scheduler, now),
            now,
            failed(FailureKind::Auth, None),
        );
        now += auth.next_attempt_in;
        let success = finish_with(only_claim(&scheduler, now), now, Outcome::Success);

        assert_eq!(network.failures, 1);
        assert_eq!(auth.failures, 1);
        assert_eq!(auth.last_failure, Some(FailureKind::Auth));
        assert_eq!(auth.next_attempt_in, secs(30));
        assert_eq!(success.failures, 0);
        assert_eq!(success.last_failure, None);
        assert_eq!(success.next_attempt_in, secs(30));
    }

    #[test]
    fn the_outage_floor_delays_but_still_lets_a_probe_through() {
        let (scheduler, _) = scheduler_with(&[(1, 10)]);
        let floors = HashMap::from([(1, secs(120))]);
        let start = Instant::now();
        finish_with(only_claim(&scheduler, start), start, Outcome::Success);

        assert!(scheduler.claim_due(start + secs(10), &floors).is_empty());
        assert!(scheduler.claim_due(start + secs(119), &floors).is_empty());
        assert_eq!(
            scheduler.next_due_in(1, start + secs(10), Some(secs(120))),
            Some(secs(110))
        );
        assert_eq!(
            scheduler.next_due_in(1, start + secs(10), None),
            Some(Duration::ZERO)
        );
        let probe = scheduler.claim_due(start + secs(120), &floors);

        assert_eq!(probe.len(), 1);
        let view = finish_with(
            probe.into_iter().next().unwrap(),
            start + secs(120),
            Outcome::Success,
        );
        assert_eq!(view.failures, 0);
        assert_eq!(view.next_attempt_in, secs(10));
    }

    #[test]
    fn interval_zero_is_never_scheduled_but_can_be_forced() {
        let (scheduler, generations) = scheduler_with(&[(1, 0)]);
        let now = Instant::now();

        assert!(scheduler
            .claim_due(now + secs(86_400), &no_floors())
            .is_empty());
        assert!(matches!(
            scheduler.claim_forced(1, generations[&1], now),
            ForcedClaim::Claimed(_)
        ));
    }

    #[test]
    fn forced_claims_bypass_next_attempt_and_the_outage_floor() {
        let (scheduler, generations) = scheduler_with(&[(1, 600)]);
        let start = Instant::now();
        finish_with(only_claim(&scheduler, start), start, Outcome::Success);
        let later = start + secs(20);

        assert!(scheduler.claim_due(later, &no_floors()).is_empty());
        assert!(matches!(
            scheduler.claim_forced(1, generations[&1], later),
            ForcedClaim::Claimed(_)
        ));
    }

    #[test]
    fn forced_claims_wait_out_a_rate_limit_and_the_minimum_gap() {
        let (scheduler, generations) = scheduler_with(&[(1, 30), (2, 30)]);
        let start = Instant::now();
        let mut claims = scheduler.claim_due(start, &no_floors());
        let second = claims.pop().unwrap();
        let first = claims.pop().unwrap();
        finish_with(
            first,
            start,
            failed(FailureKind::RateLimited, Some(secs(120))),
        );
        finish_with(second, start, Outcome::Success);

        let during_rate_limit = scheduler.claim_forced(1, generations[&1], start + secs(60));
        let within_gap = scheduler.claim_forced(2, generations[&2], start + secs(9));
        let after_gap = scheduler.claim_forced(2, generations[&2], start + secs(10));
        let after_rate_limit = scheduler.claim_forced(1, generations[&1], start + secs(120));

        assert!(matches!(
            during_rate_limit,
            ForcedClaim::Recent(Outcome::Failed {
                kind: FailureKind::RateLimited,
                ..
            })
        ));
        assert!(matches!(within_gap, ForcedClaim::Recent(Outcome::Success)));
        assert!(matches!(after_gap, ForcedClaim::Claimed(_)));
        assert!(matches!(after_rate_limit, ForcedClaim::Claimed(_)));
    }

    #[tokio::test]
    async fn forced_claims_join_an_attempt_from_claim_time_until_it_completes() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let claim = only_claim(&scheduler, now);

        let ForcedClaim::Joined(joiner) = scheduler.claim_forced(1, generations[&1], now) else {
            panic!("expected to join the in-flight attempt");
        };
        let waiting = tokio::spawn(joiner.wait());
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        let ForcedClaim::Joined(late) = scheduler.claim_forced(1, generations[&1], now) else {
            panic!("expected to join the in-flight attempt");
        };
        claim.complete(now, Outcome::Success, FetchResult::Committed(Ok(3)));

        assert!(matches!(
            waiting.await.unwrap(),
            FetchResult::Committed(Ok(3))
        ));
        assert!(matches!(late.wait().await, FetchResult::Committed(Ok(3))));
        assert!(!scheduler.view(1, now).unwrap().in_flight);
    }

    #[tokio::test]
    async fn a_dropped_claim_cancels_its_joiners_without_counting_a_failure() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let claim = only_claim(&scheduler, now);
        let ForcedClaim::Joined(joiner) = scheduler.claim_forced(1, generations[&1], now) else {
            panic!("expected to join the in-flight attempt");
        };

        drop(claim);

        assert!(matches!(joiner.wait().await, FetchResult::Cancelled));
        let view = scheduler.view(1, now).unwrap();
        assert_eq!(view.failures, 0);
        assert_eq!(view.last_failure, None);
        assert!(!view.in_flight);
        let remaining = scheduler.view(1, Instant::now()).unwrap().next_attempt_in;
        assert!(remaining <= secs(30));
        assert!(remaining > Duration::ZERO);
    }

    #[test]
    fn an_old_claim_changes_nothing_after_reset() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let old = only_claim(&scheduler, now);
        let fresh = Generation::next();
        scheduler.reset(1, fresh, secs(30));
        let current = only_claim(&scheduler, Instant::now());
        let before = scheduler.view(1, now).unwrap();

        let stale = old.complete(
            now,
            failed(FailureKind::Network, None),
            FetchResult::Superseded,
        );
        let after = scheduler.view(1, now).unwrap();

        assert!(stale.is_none());
        assert_eq!(after, before);
        assert!(after.in_flight);
        assert_eq!(after.failures, 0);
        assert_ne!(generations[&1], fresh);
        assert_eq!(current.generation(), fresh);
    }

    #[test]
    fn an_old_claim_dropped_after_reset_leaves_the_new_attempt_alone() {
        let (scheduler, _) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let old = only_claim(&scheduler, now);
        scheduler.reset(1, Generation::next(), secs(30));
        let current = only_claim(&scheduler, Instant::now());

        drop(old);

        assert!(scheduler.view(1, now).unwrap().in_flight);
        drop(current);
        assert!(!scheduler.view(1, now).unwrap().in_flight);
    }

    #[test]
    fn claims_after_remove_are_refused_and_the_entry_is_not_recreated() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let old = only_claim(&scheduler, now);
        scheduler.remove(1);

        assert!(matches!(
            scheduler.claim_forced(1, generations[&1], now),
            ForcedClaim::Refused
        ));
        assert!(scheduler.claim_due(now, &no_floors()).is_empty());
        old.complete(now, Outcome::Success, FetchResult::Superseded);
        assert!(scheduler.view(1, now).is_none());
        assert!(scheduler.claim_due(now, &no_floors()).is_empty());
        scheduler.set_interval(1, generations[&1], secs(5));
        assert!(scheduler.view(1, now).is_none());
    }

    #[test]
    fn a_reused_id_with_a_new_generation_ignores_old_claims() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let old = only_claim(&scheduler, now);
        scheduler.remove(1);
        let reused = Generation::next();
        scheduler.register(1, reused, secs(30));

        assert!(matches!(
            scheduler.claim_forced(1, generations[&1], now),
            ForcedClaim::Refused
        ));
        old.complete(
            now,
            failed(FailureKind::Network, None),
            FetchResult::Superseded,
        );
        let view = scheduler.view(1, now).unwrap();

        assert_eq!(view.failures, 0);
        assert!(!view.in_flight);
        assert!(matches!(
            scheduler.claim_forced(1, reused, now),
            ForcedClaim::Claimed(_)
        ));
    }

    #[test]
    fn register_keeps_an_existing_entry_of_the_same_generation() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        finish_with(
            only_claim(&scheduler, now),
            now,
            failed(FailureKind::Network, None),
        );

        scheduler.register(1, generations[&1], secs(30));

        assert_eq!(scheduler.view(1, now).unwrap().failures, 1);
    }

    #[test]
    fn a_shorter_interval_brings_the_next_attempt_forward() {
        let (scheduler, generations) = scheduler_with(&[(1, 3600)]);
        let now = Instant::now();
        finish_with(only_claim(&scheduler, now), now, Outcome::Success);

        scheduler.set_interval(1, generations[&1], secs(30));

        assert!(scheduler.view(1, Instant::now()).unwrap().next_attempt_in <= secs(30));
        assert_eq!(due_ids(&scheduler, now + secs(30)), vec![1]);
    }

    #[test]
    fn a_quota_pause_postpones_every_outcome_until_the_reset() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let wall = Utc::now();
        let reset = wall + chrono::Duration::minutes(20);
        let claim = only_claim(&scheduler, now);

        let changed = scheduler.set_quota_pause(1, generations[&1], Some(reset), now, wall);
        let repeated = scheduler.set_quota_pause(1, generations[&1], Some(reset), now, wall);
        let view = claim.complete(now, Outcome::Cancelled, FetchResult::Cancelled);

        assert!(changed);
        assert!(!repeated);
        assert_eq!(view.unwrap().next_attempt_in, secs(20 * 60));
        assert_eq!(scheduler.paused_until(1, wall), Some(reset));
        assert!(scheduler
            .claim_due(now + secs(20 * 60 - 1), &no_floors())
            .is_empty());
        assert_eq!(due_ids(&scheduler, now + secs(20 * 60)), vec![1]);
    }

    #[test]
    fn a_quota_pause_turns_manual_refreshes_into_a_rate_limit_answer() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let wall = Utc::now();
        scheduler.set_quota_pause(
            1,
            generations[&1],
            Some(wall + chrono::Duration::minutes(5)),
            now,
            wall,
        );

        let forced = scheduler.claim_forced(1, generations[&1], now + secs(60));

        let ForcedClaim::Recent(Outcome::Failed { kind, retry_after }) = forced else {
            panic!("expected a recent rate-limit answer, got {forced:?}");
        };
        assert_eq!(kind, FailureKind::RateLimited);
        assert_eq!(retry_after, Some(secs(240)));
    }

    #[test]
    fn a_quota_pause_clears_when_the_quota_is_back_and_ignores_stale_generations() {
        let (scheduler, generations) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();
        let wall = Utc::now();
        let reset = wall + chrono::Duration::minutes(5);
        let stale = Generation::next();

        let ignored = scheduler.set_quota_pause(1, stale, Some(reset), now, wall);
        scheduler.set_quota_pause(1, generations[&1], Some(reset), now, wall);
        let cleared = scheduler.set_quota_pause(1, generations[&1], None, now, wall);
        let already_clear = scheduler.set_quota_pause(1, generations[&1], None, now, wall);

        assert!(!ignored);
        assert!(cleared);
        assert!(!already_clear);
        assert_eq!(scheduler.paused_until(1, wall), None);
        let past = wall - chrono::Duration::seconds(1);
        assert!(!scheduler.set_quota_pause(1, generations[&1], Some(past), now, wall));
        assert_eq!(scheduler.paused_until(1, wall), None);
    }

    #[test]
    fn generations_are_never_reused() {
        let first = Generation::next();
        let second = Generation::next();

        assert_ne!(first, second);
        assert!(second > first);
    }
}
