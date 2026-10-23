mod alerts;
mod feed;
mod signals;
#[cfg(test)]
mod tests;

use std::collections::{
    HashMap,
    HashSet,
};
use std::future::pending;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    ComponentHealth,
    FeedFormat,
    StatusSource,
};
use reqwest::Url;
use tokio::sync::{
    mpsc,
    watch,
    Mutex,
    Semaphore,
};
use tokio::task::{
    JoinError,
    JoinHandle,
    JoinSet,
};
use tokio::time::{
    sleep_until,
    timeout,
    Instant,
};
use tokio_util::sync::CancellationToken;

use self::alerts::{
    connection_candidate,
    connection_recovery,
    feed_candidates,
    feed_recovery,
    instance_candidates,
    instance_recovery,
    merge,
    Alert,
    AlertTracker,
    Observation,
    CALM_CHECKS,
    CALM_FETCHES,
};
use self::feed::{
    fetch_feed,
    FeedOutcome,
    FEED_INVALID,
    FEED_UNREACHABLE,
};
use self::signals::{
    next_evidence_change,
    Evidence,
    Observed,
    UnavailableReason,
};
use super::provider_service::ProviderService;
use crate::application::fetch_scheduler::{
    ForcedClaim,
    Generation,
    ScheduleView,
};
use crate::application::refresh_manager::RefreshMode;
use crate::domain::{
    DomainError,
    EvidenceView,
    FailureKind,
    HealthLevel,
    HealthSignal,
    SignalKind,
    StatusPageRef,
};
use crate::event::EventBus;
use crate::infrastructure::status_feeds::{
    evaluate,
    ParsedFeed,
};
use crate::infrastructure::{
    ConfigKey,
    ConfigState,
    HttpClientManager,
};

const FEED_INTERVAL: Duration = Duration::from_secs(120);
const FEED_INTERVAL_FAILING: Duration = Duration::from_secs(60);
const ACTIVATION_MAX_AGE: Duration = Duration::from_secs(60);
const FEED_BACKOFF_BASE: Duration = Duration::from_secs(60);
const FEED_BACKOFF_MAX: Duration = Duration::from_secs(30 * 60);
const INSTANCE_INTERVAL: Duration = Duration::from_secs(120);
const INSTANCE_TIMEOUT: Duration = Duration::from_secs(10);
const INSTANCE_CONCURRENCY: usize = 4;

static NEXT_EPISODE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum HealthInput {
    FetchOutcome {
        provider_id: i64,
        generation: Generation,
        view: ScheduleView,
        kind: Option<FailureKind>,
    },
    ProvidersChanged {
        changed: Vec<ProviderHealthInput>,
    },
    ForceFeeds {
        min_age: Duration,
    },
    SettingsChanged,
}

#[derive(Debug, Clone)]
pub struct ProviderHealthInput {
    pub id: i64,
    pub generation: Option<Generation>,
    pub display_name: String,
    pub source: Option<StatusSource>,
    pub has_instance_checks: bool,
}

#[derive(Debug, Default)]
pub struct HealthSnapshot {
    pub providers: HashMap<i64, ProviderHealthState>,
}

#[derive(Debug, Clone)]
pub struct ProviderHealthState {
    pub generation: Generation,
    pub level: HealthLevel,
    pub signals: Vec<HealthSignal>,
    pub status_page: Option<StatusPageRef>,
    pub outage_floor: bool,
}

#[derive(Clone)]
pub struct HealthHandle {
    tx: mpsc::UnboundedSender<HealthInput>,
    snapshot: watch::Receiver<Arc<HealthSnapshot>>,
}

#[derive(Debug)]
pub struct HealthReceivers {
    pub inputs: mpsc::UnboundedReceiver<HealthInput>,
    pub snapshot: watch::Sender<Arc<HealthSnapshot>>,
}

impl HealthHandle {
    pub fn channel() -> (Self, HealthReceivers) {
        let (tx, inputs) = mpsc::unbounded_channel();
        let (snapshot_tx, snapshot) = watch::channel(Arc::new(HealthSnapshot::default()));

        (
            Self { tx, snapshot },
            HealthReceivers {
                inputs,
                snapshot: snapshot_tx,
            },
        )
    }

    pub fn send(&self, input: HealthInput) {
        let _ = self.tx.send(input);
    }

    pub fn snapshot(&self) -> Arc<HealthSnapshot> {
        Arc::clone(&self.snapshot.borrow())
    }

    pub fn floors(&self, outage_floor: Duration) -> HashMap<i64, Duration> {
        self.snapshot
            .borrow()
            .providers
            .iter()
            .filter(|(_, state)| state.outage_floor)
            .map(|(id, _)| (*id, outage_floor))
            .collect()
    }
}

type FeedKey = (Url, FeedFormat);

fn feed_key(source: &StatusSource) -> FeedKey {
    (source.feed_url.clone(), source.format)
}

fn feed_subject(key: &FeedKey) -> String {
    format!("status-page:{:?}:{}", key.1, key.0)
}

fn instance_subject(id: i64) -> String {
    format!("instance:{id}")
}

fn connection_subject(id: i64) -> String {
    format!("connection:{id}")
}

fn next_episode() -> String {
    format!("episode-{}", NEXT_EPISODE.fetch_add(1, Ordering::Relaxed))
}

fn episode_for(episodes: &mut HashMap<SignalKind, String>, kind: SignalKind) -> String {
    episodes.entry(kind).or_insert_with(next_episode).clone()
}

struct FeedState {
    etag: Option<String>,
    last_good: Option<ParsedFeed>,
    observed: Option<Observed>,
    failures: u32,
    next_try: Instant,
    last_attempt: Option<Instant>,
    in_flight: Option<u64>,
    error: Option<&'static str>,
    force: bool,
    fresh: bool,
    logged_unknown: bool,
    logged_fallback: bool,
}

impl FeedState {
    fn new(now: Instant) -> Self {
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

    fn failed(&mut self, now: Instant, message: &'static str, retry_after: Option<Duration>) {
        self.failures = self.failures.saturating_add(1);
        self.error = Some(message);
        let backoff = FEED_BACKOFF_BASE
            .saturating_mul(1u32 << (self.failures - 1).min(10))
            .min(FEED_BACKOFF_MAX);
        self.next_try = now + backoff.max(retry_after.unwrap_or_default());
    }
}

fn feed_evidence(feed: &FeedState, now: Instant) -> Option<Evidence> {
    Evidence::of(feed.observed, None, now).or_else(|| {
        feed.error
            .map(|_| Evidence::Unavailable(UnavailableReason::FeedError))
    })
}

struct InstanceState {
    components: Vec<ComponentHealth>,
    observed: Option<Observed>,
    unavailable: Option<UnavailableReason>,
    in_flight: Option<u64>,
    next_check: Instant,
    fresh: bool,
}

impl InstanceState {
    fn new(now: Instant) -> Self {
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

struct ConnectionOutcome {
    kind: Option<FailureKind>,
    failures: u32,
    at: DateTime<Utc>,
}

struct ProviderEntry {
    generation: Generation,
    display_name: String,
    source: Option<StatusSource>,
    has_instance_checks: bool,
    instance: InstanceState,
    outcome: Option<ConnectionOutcome>,
    last_fetch_ok: bool,
    episodes: HashMap<SignalKind, String>,
    feed_level: Option<HealthLevel>,
}

impl ProviderEntry {
    fn new(
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

    fn instance_due(&self, now: Instant) -> bool {
        self.has_instance_checks
            && self.last_fetch_ok
            && self.instance.in_flight.is_none()
            && now >= self.instance.next_check
    }

    fn failing_transiently(&self) -> bool {
        self.outcome
            .as_ref()
            .and_then(|outcome| outcome.kind)
            .is_some_and(FailureKind::is_transient)
    }
}

enum CheckOutcome {
    Components(Vec<ComponentHealth>),
    Denied,
    NotInstalled,
    Failed,
}

enum TaskResult {
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

async fn run_check(
    service: Arc<ProviderService>, permits: Arc<Semaphore>, id: i64, generation: Generation,
) -> CheckOutcome {
    let Ok(_permit) = permits.acquire_owned().await else {
        return CheckOutcome::Failed;
    };
    let lease = match service.acquire(id).await {
        Ok(lease) if lease.generation == generation => lease,
        Ok(_) => return CheckOutcome::Failed,
        Err(e) => {
            tracing::debug!(provider_id = id, error = %e, "Controller health check could not load the provider");
            return CheckOutcome::Failed;
        }
    };

    match timeout(INSTANCE_TIMEOUT, lease.provider.check_health()).await {
        Ok(Ok(components)) => CheckOutcome::Components(components),
        Ok(Err(DomainError::PermissionDenied(_))) => CheckOutcome::Denied,
        Ok(Err(DomainError::PipelineNotFound(_) | DomainError::NotFound(_))) => {
            CheckOutcome::NotInstalled
        }
        Ok(Err(e)) => {
            tracing::debug!(provider_id = id, error = %e, "Controller health check failed");
            CheckOutcome::Failed
        }
        Err(_) => {
            tracing::debug!(provider_id = id, "Controller health check timed out");
            CheckOutcome::Failed
        }
    }
}

fn comparable(
    state: &ProviderHealthState,
) -> (HealthLevel, Vec<HealthSignal>, Option<StatusPageRef>) {
    (
        state.level,
        state
            .signals
            .iter()
            .map(|signal| HealthSignal {
                observed_at: DateTime::UNIX_EPOCH,
                updated_at: DateTime::UNIX_EPOCH,
                ..signal.clone()
            })
            .collect(),
        state.status_page.as_ref().map(|page| StatusPageRef {
            checked_at: None,
            ..page.clone()
        }),
    )
}

fn same_health(a: &ProviderHealthState, b: &ProviderHealthState) -> bool {
    comparable(a) == comparable(b)
}

fn default_state(generation: Generation) -> ProviderHealthState {
    ProviderHealthState {
        generation,
        level: HealthLevel::Operational,
        signals: Vec::new(),
        status_page: None,
        outage_floor: false,
    }
}

fn derive(
    entry: &mut ProviderEntry, feeds: &mut HashMap<FeedKey, FeedState>, enabled: bool,
    now: Instant, wall: DateTime<Utc>,
) -> ProviderHealthState {
    let mut signals: Vec<HealthSignal> = Vec::new();
    let mut status_page = None;
    let mut present: Vec<SignalKind> = Vec::new();

    if let Some(source) = &entry.source {
        if let Some(feed) = feeds.get_mut(&feed_key(source)) {
            let evidence = if enabled {
                feed_evidence(feed, now)
            } else {
                Some(Evidence::Suspended)
            };
            if enabled {
                status_page = Some(StatusPageRef {
                    name: source.name.clone(),
                    url: source.page_url.to_string(),
                    checked_at: feed.observed.map(|observed| observed.wall),
                    error: feed.error.map(str::to_owned),
                    components: Vec::new(),
                    other_incidents: Vec::new(),
                });
            }
            if let (Some(parsed), Some((view, observed))) =
                (&feed.last_good, evidence.and_then(Evidence::shown))
            {
                let eval = evaluate(parsed, &source.components, &source.page_url, wall);
                if let Some(page) = status_page.as_mut() {
                    page.components = eval.watched.clone();
                    page.other_incidents = eval.other_incidents.clone();
                }
                if eval.filter_fallback && !feed.logged_fallback {
                    feed.logged_fallback = true;
                    tracing::warn!(
                        source = %source.name,
                        "None of the watched component ids exist in the status feed, watching all components"
                    );
                }
                if let Some(mut signal) = signals::status_page(source, &eval, "", view, observed) {
                    signal.episode_id = episode_for(&mut entry.episodes, SignalKind::StatusPage);
                    present.push(SignalKind::StatusPage);
                    signals.push(signal);
                }
            }
        }
    }

    if entry.has_instance_checks {
        let evidence = Evidence::of(entry.instance.observed, entry.instance.unavailable, now);
        if let Some((view, observed)) = evidence.and_then(Evidence::shown) {
            if let Some(mut signal) = signals::instance(
                &entry.display_name,
                &entry.instance.components,
                "",
                view,
                observed,
            ) {
                signal.episode_id = episode_for(&mut entry.episodes, SignalKind::Instance);
                present.push(SignalKind::Instance);
                signals.push(signal);
            }
        }
    }

    if let Some(outcome) = &entry.outcome {
        if let Some(connection) = outcome
            .kind
            .and_then(|kind| signals::connection(&entry.display_name, kind, outcome.failures))
        {
            let episode = episode_for(&mut entry.episodes, connection.kind);
            present.push(connection.kind);
            signals.push(signals::connection_signal(
                &connection,
                &episode,
                outcome.at,
            ));
        }
    }
    entry.episodes.retain(|kind, _| present.contains(kind));

    signals.sort_by(|a, b| {
        b.level.cmp(&a.level).then_with(|| {
            (a.evidence == EvidenceView::Stale).cmp(&(b.evidence == EvidenceView::Stale))
        })
    });
    let level = signals
        .iter()
        .filter(|signal| signal.evidence == EvidenceView::Fresh)
        .map(|signal| signal.level)
        .max()
        .unwrap_or(HealthLevel::Operational);

    ProviderHealthState {
        generation: entry.generation,
        level,
        signals,
        status_page,
        outage_floor: level == HealthLevel::MajorOutage,
    }
}

struct Actor {
    provider_service: Arc<ProviderService>,
    http: Arc<HttpClientManager>,
    event_bus: Arc<dyn EventBus>,
    config: Arc<ConfigState>,
    snapshot: watch::Sender<Arc<HealthSnapshot>>,
    check_permits: Arc<Semaphore>,
    enabled: bool,
    active: bool,
    feeds: HashMap<FeedKey, FeedState>,
    providers: HashMap<i64, ProviderEntry>,
    alerts: AlertTracker,
    published: HashMap<i64, ProviderHealthState>,
    outbox: Vec<Alert>,
    recoveries: Vec<(i64, Generation)>,
    tasks: JoinSet<TaskResult>,
    next_token: u64,
}

impl Actor {
    async fn handle(&mut self, input: HealthInput) {
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

    async fn finish(&mut self, joined: Result<TaskResult, JoinError>) {
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

    async fn set_active(&mut self, active: bool) {
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
        let candidates = match kind {
            None => Vec::new(),
            Some(kind) if !kind.is_transient() => {
                self.alerts.reset(&subject);
                return;
            }
            Some(kind) => match signals::connection(&entry.display_name, kind, view.failures) {
                Some(connection) if connection.kind == SignalKind::Connection => {
                    vec![connection_candidate(&connection, id)]
                }
                _ => return,
            },
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

    fn on_feed(&mut self, key: &FeedKey, token: u64, outcome: FeedOutcome, now: Instant) {
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
        let Some(recovery) = recovery else {
            return;
        };

        let alerts = self.alerts.observe(
            &feed_subject(key),
            now,
            Observation {
                candidates: merge(candidates),
                recovery,
                provider_ids: users,
                calm_needed: CALM_CHECKS,
            },
        );
        self.outbox.extend(alerts);
        self.recoveries.extend(recovered);
    }

    fn on_instance(
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
        let permits = Arc::clone(&self.check_permits);

        self.tasks.spawn(async move {
            let outcome = run_check(service, permits, id, generation).await;

            TaskResult::Instance {
                id,
                generation,
                token,
                outcome,
            }
        });
    }

    fn next_wake(&self, now: Instant) -> Option<Instant> {
        let mut wake: Option<Instant> = None;
        let mut note = |at: Instant| wake = Some(wake.map_or(at, |known| known.min(at)));

        if self.enabled && self.active {
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
            if self.active
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

    fn detect_gaps(&mut self, now: Instant) {
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

    async fn tick(&mut self) {
        let now = Instant::now();
        self.spawn_due(now);
        self.refresh(now).await;
    }

    async fn refresh(&mut self, now: Instant) {
        let wall = Utc::now();
        self.detect_gaps(now);

        let mut states = HashMap::with_capacity(self.providers.len());
        for (id, entry) in &mut self.providers {
            states.insert(*id, derive(entry, &mut self.feeds, self.enabled, now, wall));
        }
        let snapshot = Arc::new(HealthSnapshot { providers: states });
        self.snapshot.send_replace(Arc::clone(&snapshot));

        let mut changed: Vec<(i64, Generation)> = snapshot
            .providers
            .iter()
            .filter(|(id, state)| {
                self.published
                    .get(*id)
                    .is_none_or(|published| !same_health(published, state))
            })
            .map(|(id, state)| (*id, state.generation))
            .collect();
        changed.sort_unstable();
        for (id, generation) in changed {
            self.provider_service.publish_summary(id, generation).await;
        }
        self.published = snapshot.providers.clone();

        self.flush().await;
    }

    async fn flush(&mut self) {
        for alert in std::mem::take(&mut self.outbox) {
            let mut current = Vec::with_capacity(alert.provider_ids.len());
            for id in alert.provider_ids.iter().copied() {
                let known = self.providers.get(&id).map(|entry| entry.generation);
                if known.is_some() && self.provider_service.generation_of(id).await == known {
                    current.push(id);
                }
            }
            if current.is_empty() {
                continue;
            }
            self.event_bus.emit(alert.into_event(current)).await;
        }

        for (id, generation) in std::mem::take(&mut self.recoveries) {
            if let ForcedClaim::Claimed(claim) =
                self.provider_service
                    .scheduler()
                    .claim_forced(id, generation, Instant::now())
            {
                self.provider_service.dispatcher().fetch_now(vec![claim]);
            }
        }
    }
}

async fn wake_at(at: Option<Instant>) {
    match at {
        Some(at) => sleep_until(at).await,
        None => pending().await,
    }
}

async fn run(
    mut actor: Actor, mut inputs: mpsc::UnboundedReceiver<HealthInput>,
    mut mode: watch::Receiver<RefreshMode>, cancel: CancellationToken,
) {
    actor.enabled = actor.config.get_bool(ConfigKey::StatusPagesEnabled).await;
    actor.active = *mode.borrow_and_update() == RefreshMode::Active;
    let mut mode_open = true;
    tracing::info!(enabled = actor.enabled, "ProviderHealthService started");

    loop {
        let wake = actor.next_wake(Instant::now());
        tokio::select! {
            _ = cancel.cancelled() => break,
            input = inputs.recv() => match input {
                Some(input) => actor.handle(input).await,
                None => break,
            },
            changed = mode.changed(), if mode_open => match changed {
                Ok(()) => {
                    let active = *mode.borrow_and_update() == RefreshMode::Active;
                    actor.set_active(active).await;
                }
                Err(_) => mode_open = false,
            },
            Some(joined) = actor.tasks.join_next(), if !actor.tasks.is_empty() => {
                actor.finish(joined).await;
            }
            _ = wake_at(wake) => actor.tick().await,
        }
    }

    actor.tasks.shutdown().await;
    tracing::info!("ProviderHealthService stopped");
}

struct Parts {
    actor: Actor,
    inputs: mpsc::UnboundedReceiver<HealthInput>,
    mode: watch::Receiver<RefreshMode>,
}

pub struct ProviderHealthService {
    parts: Mutex<Option<Parts>>,
    cancel: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl ProviderHealthService {
    pub fn new(
        receivers: HealthReceivers, provider_service: Arc<ProviderService>,
        http: Arc<HttpClientManager>, event_bus: Arc<dyn EventBus>, config: Arc<ConfigState>,
        mode: watch::Receiver<RefreshMode>,
    ) -> Self {
        let actor = Actor {
            provider_service,
            http,
            event_bus,
            config,
            snapshot: receivers.snapshot,
            check_permits: Arc::new(Semaphore::new(INSTANCE_CONCURRENCY)),
            enabled: true,
            active: true,
            feeds: HashMap::new(),
            providers: HashMap::new(),
            alerts: AlertTracker::default(),
            published: HashMap::new(),
            outbox: Vec::new(),
            recoveries: Vec::new(),
            tasks: JoinSet::new(),
            next_token: 0,
        };

        Self {
            parts: Mutex::new(Some(Parts {
                actor,
                inputs: receivers.inputs,
                mode,
            })),
            cancel: CancellationToken::new(),
            task: Mutex::new(None),
        }
    }

    pub async fn start(&self) {
        let mut task = self.task.lock().await;
        if task.is_some() || self.cancel.is_cancelled() {
            return;
        }
        let Some(parts) = self.parts.lock().await.take() else {
            return;
        };

        *task = Some(tokio::spawn(run(
            parts.actor,
            parts.inputs,
            parts.mode,
            self.cancel.clone(),
        )));
    }

    pub async fn stop(&self) {
        self.cancel.cancel();
        let task = self.task.lock().await.take();
        if let Some(task) = task {
            if let Err(e) = task.await {
                tracing::error!(error = %e, "ProviderHealthService task failed");
            }
        }
    }
}

impl Drop for ProviderHealthService {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
