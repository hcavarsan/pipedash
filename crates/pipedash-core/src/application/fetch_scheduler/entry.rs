use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

use super::{
    AttemptSender,
    BackoffPolicy,
    Entry,
    Generation,
    Outcome,
    ScheduleView,
    MAX_INTERVAL,
};
use crate::domain::FailureKind;

const FAR_FUTURE: Duration = Duration::from_secs(10 * 365 * 24 * 60 * 60);

impl Entry {
    pub(super) fn new(generation: Generation, interval: Duration, now: Instant) -> Self {
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
            forced_waiters: 0,
        }
    }

    pub(super) fn view(&self, now: Instant) -> ScheduleView {
        ScheduleView {
            failures: self.failures,
            last_failure: self.last_failure,
            next_attempt_in: self.next_attempt.saturating_duration_since(now),
            in_flight: self.attempt.is_some(),
        }
    }

    pub(super) fn due_at(&self, floor: Option<Duration>) -> Instant {
        match (floor, self.last_attempt) {
            (Some(floor), Some(last)) => self.next_attempt.max(after(last, floor)),
            _ => self.next_attempt,
        }
    }

    pub(super) fn forced_delay(&self, policy: &BackoffPolicy, now: Instant) -> Duration {
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

    pub(super) fn start_attempt(&mut self, now: Instant) -> (AttemptSender, Option<Instant>) {
        let (sender, receiver) = watch::channel(None);
        self.attempt = Some(receiver);
        (sender, self.last_attempt.replace(now))
    }

    pub(super) fn release(&mut self, previous_attempt: Option<Instant>) {
        self.attempt = None;
        self.last_attempt = previous_attempt;
    }

    pub(super) fn finish(&mut self, policy: &BackoffPolicy, now: Instant, outcome: Outcome) {
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

pub(super) fn after(now: Instant, wait: Duration) -> Instant {
    now.checked_add(wait).unwrap_or_else(|| now + FAR_FUTURE)
}
