use std::sync::atomic::AtomicU64;
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use self::live::{
    Interest,
    Live,
    RunKey,
};
use super::presenter::Presenter;
use super::provider_service::{
    ProviderLease,
    ProviderService,
};
use crate::application::Generation;
use crate::domain::{
    DomainError,
    DomainResult,
    LogCacheStats,
    PipelineRun,
    RunLogNode,
    RunLogTree,
};
use crate::event::EventBus;
use crate::infrastructure::database::Repository;
use crate::infrastructure::deduplication::RequestDeduplicator;
use crate::infrastructure::run_logs::{
    CacheEpoch,
    DocKey,
    Document,
    LogStore,
    ReadLease,
    StepBound,
};

mod access;
mod documents;
mod export;
mod ingest;
mod live;
mod read;
#[cfg(test)]
mod tests;
mod tree;

const INGEST_PERMITS: usize = 2;
const SEARCH_PERMITS: usize = 2;
const INGEST_QUEUE: usize = 16;
const EXPORT_QUEUE: usize = 4;
const PROGRESS_TICK: Duration = Duration::from_millis(500);
const COMPLETE_TREE_TTL: Duration = Duration::from_secs(60);
const OPEN_TREE_TTL: Duration = Duration::from_secs(10);
const FAILURE_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const INTEREST_TTL: Duration = Duration::from_secs(45);
const TREE_INTERVAL: Duration = Duration::from_secs(5);
const TREE_BACKOFF_MAX: Duration = Duration::from_secs(60);
const RESUMABLE_INTERVAL: Duration = Duration::from_secs(2);
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(10);
const SHUTDOWN_PATIENCE: Duration = Duration::from_secs(5);
const REVISION_GONE: &str = "This log revision is no longer available";
const EXPORT_INTERRUPTED: &str = "Pipedash stopped before the log finished downloading";
const PROVIDER_CHANGED: &str = "The provider changed while the log was downloading. Open it again.";
const PROVIDER_MISBEHAVED: &str =
    "The provider returned the log in an unexpected way. Try again later.";

type TreeKey = (i64, Generation, CacheEpoch, String, i64);
type Built = (Arc<RunLogTree>, Arc<PipelineRun>);

struct Attempt {
    id: u64,
    generation: Generation,
    epoch: CacheEpoch,
    received: Arc<AtomicU64>,
    cancel: CancellationToken,
}

struct AttemptRecord {
    generation: Generation,
    epoch: CacheEpoch,
    at: Instant,
    failure: Option<String>,
    resumable: bool,
}

enum Stop {
    Silent,
    ProviderChanged,
}

enum Finished {
    Stopped(Stop),
    Published { revision: i64, resumable: bool },
    Unchanged { resumable: bool },
    Failed(String),
}

struct AttemptGuard {
    service: Arc<LogService>,
    key: DocKey,
    id: u64,
}

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        self.service
            .inflight
            .remove_if(&self.key, |_, attempt| attempt.id == self.id);
    }
}

struct SearchSession<'a> {
    searches: &'a DashMap<(DocKey, String), (u64, CancellationToken)>,
    key: (DocKey, String),
    id: u64,
    cancel: CancellationToken,
}

impl Drop for SearchSession<'_> {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.searches
            .remove_if(&self.key, |_, (id, _)| *id == self.id);
    }
}

pub struct LogService {
    store: Arc<LogStore>,
    provider_service: Arc<ProviderService>,
    repository: Arc<Repository>,
    event_bus: Arc<dyn EventBus>,
    presenter: Arc<Presenter>,
    trees: RequestDeduplicator<Built, DomainError>,
    tree_memo: DashMap<TreeKey, (Instant, Arc<RunLogTree>, Arc<PipelineRun>)>,
    pipeline_providers: DashMap<String, i64>,
    inflight: DashMap<DocKey, Attempt>,
    attempts: DashMap<DocKey, AttemptRecord>,
    searches: DashMap<(DocKey, String), (u64, CancellationToken)>,
    interests: DashMap<RunKey, Interest>,
    lives: DashMap<RunKey, Live>,
    ingest_permits: Arc<Semaphore>,
    search_permits: Arc<Semaphore>,
    admission: Mutex<bool>,
    root: CancellationToken,
    tracker: TaskTracker,
    next_id: AtomicU64,
}

pub struct LogExport {
    doc: Arc<Document>,
    lease: ReadLease,
    provider_id: i64,
    epoch: CacheEpoch,
    service: Arc<LogService>,
}

struct IngestWork {
    service: Arc<LogService>,
    lease: ProviderLease,
    epoch: CacheEpoch,
    key: DocKey,
    node: RunLogNode,
    run: Arc<PipelineRun>,
    steps: Vec<StepBound>,
    revision: i64,
    cancel: CancellationToken,
    received: Arc<AtomicU64>,
}

impl LogService {
    pub(crate) fn new(
        store: Arc<LogStore>, provider_service: Arc<ProviderService>, repository: Arc<Repository>,
        event_bus: Arc<dyn EventBus>, presenter: Arc<Presenter>,
    ) -> Self {
        Self {
            store,
            provider_service,
            repository,
            event_bus,
            presenter,
            trees: RequestDeduplicator::new(),
            tree_memo: DashMap::new(),
            pipeline_providers: DashMap::new(),
            inflight: DashMap::new(),
            attempts: DashMap::new(),
            interests: DashMap::new(),
            lives: DashMap::new(),
            searches: DashMap::new(),
            ingest_permits: Arc::new(Semaphore::new(INGEST_PERMITS)),
            search_permits: Arc::new(Semaphore::new(SEARCH_PERMITS)),
            admission: Mutex::new(true),
            root: CancellationToken::new(),
            tracker: TaskTracker::new(),
            next_id: AtomicU64::new(1),
        }
    }

    fn admit(&self, spawn: impl FnOnce(&TaskTracker)) -> DomainResult<()> {
        let open = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !*open {
            return Err(shutting_down());
        }
        spawn(&self.tracker);

        Ok(())
    }

    pub async fn stats(&self) -> LogCacheStats {
        self.store.stats().await
    }

    pub async fn clear(&self) -> DomainResult<()> {
        let cleared = self.store.clear().await;
        for attempt in self.inflight.iter() {
            attempt.cancel.cancel();
        }
        for live in self.lives.iter() {
            live.cancel.cancel();
        }
        for search in self.searches.iter() {
            search.value().1.cancel();
        }
        self.interests.clear();
        self.tree_memo.clear();
        self.attempts.clear();
        self.searches.clear();

        cleared
    }

    pub async fn shutdown(&self) {
        {
            let mut open = self
                .admission
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *open = false;
            self.root.cancel();
        }
        self.tracker.close();
        if tokio::time::timeout(SHUTDOWN_PATIENCE, self.tracker.wait())
            .await
            .is_err()
        {
            tracing::warn!("Waiting for run log tasks to stop");
            self.tracker.wait().await;
        }
    }
}

fn failure_message(error: &DomainError) -> String {
    match error {
        DomainError::PipelineNotFound(_) | DomainError::NotFound(_) => {
            "The provider no longer has this log. It may have expired.".to_string()
        }
        DomainError::RateLimited {
            retry_after: Some(wait),
            ..
        } => format!(
            "Rate limit reached. Try again in {} min.",
            wait.as_millis().div_ceil(60_000).max(1)
        ),
        DomainError::RateLimited {
            retry_after: None, ..
        } => "Rate limit reached. Try again later.".to_string(),
        other => other.to_string(),
    }
}

fn shutting_down() -> DomainError {
    DomainError::InternalError("Pipedash is shutting down".to_string())
}
