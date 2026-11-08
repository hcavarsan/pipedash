mod alert_candidates;
mod alerts;
mod checks;
mod derive;
mod feed;
mod inputs;
mod notifications;
mod observations;
mod publish;
mod runtime;
mod schedule;
mod signals;
mod state;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use pipedash_plugin_api::{
    FeedFormat,
    StatusSource,
};
use reqwest::Url;
use tokio::sync::{
    mpsc,
    watch,
};
use tokio::task::{
    JoinHandle,
    JoinSet,
};
use tokio_util::sync::CancellationToken;

use self::alerts::{
    Alert,
    AlertTracker,
};
pub use self::notifications::{
    refresh_notifications,
    wanted_notifications,
    LocalNotification,
    NotificationState,
    ObservedNotification,
    ShownNotification,
};
use self::state::{
    FeedState,
    ProviderEntry,
    TaskResult,
};
use super::provider_service::ProviderService;
use crate::application::fetch_scheduler::{
    Generation,
    ScheduleView,
};
use crate::application::refresh_manager::RefreshMode;
use crate::domain::{
    FailureKind,
    HealthLevel,
    HealthSignal,
    SignalKind,
    StatusPageRef,
};
use crate::event::EventBus;
use crate::infrastructure::{
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
const JOB_CONCURRENCY: usize = 4;
const FEED_SUBJECT_PREFIX: &str = "status-page:";

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
    format!("{FEED_SUBJECT_PREFIX}{:?}:{}", key.1, key.0)
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

struct Actor {
    provider_service: Arc<ProviderService>,
    http: Arc<HttpClientManager>,
    event_bus: Arc<dyn EventBus>,
    config: Arc<ConfigState>,
    snapshot: watch::Sender<Arc<HealthSnapshot>>,
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
