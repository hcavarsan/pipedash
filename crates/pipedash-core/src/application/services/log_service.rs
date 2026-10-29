use std::collections::{
    HashMap,
    HashSet,
};
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};
use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use dashmap::{
    DashMap,
    Entry,
};
use futures::Stream;
use tokio::sync::{
    mpsc,
    oneshot,
    Semaphore,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use self::live::{
    Interest,
    Live,
    RunKey,
};
use super::provider_service::{
    ProviderLease,
    ProviderService,
};
use crate::application::Generation;
use crate::domain::{
    DomainError,
    DomainResult,
    LogAvailability,
    LogCacheStats,
    LogLinesPage,
    LogNode,
    LogNodeKind,
    LogReadMode,
    LogSearchQuery,
    LogSearchResult,
    LogSink,
    PipelineRun,
    PipelineStatus,
    RunLogDocument,
    RunLogNode,
    RunLogTree,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::Repository;
use crate::infrastructure::deduplication::{
    hash_request,
    RequestDeduplicator,
};
use crate::infrastructure::run_logs::{
    search_regex,
    Base,
    BlockLimits,
    CacheEpoch,
    DocError,
    DocKey,
    Document,
    Ingester,
    LogStore,
    PublishOutcome,
    ReadLease,
    StepBound,
    StoredDocMeta,
    StoredTree,
};

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
    pub fn new(
        store: Arc<LogStore>, provider_service: Arc<ProviderService>, repository: Arc<Repository>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            store,
            provider_service,
            repository,
            event_bus,
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
            return Err(DomainError::InternalError(
                "Pipedash is shutting down".to_string(),
            ));
        }
        spawn(&self.tracker);

        Ok(())
    }

    async fn provider_of(&self, pipeline_id: &str) -> DomainResult<i64> {
        if let Some(provider_id) = self.pipeline_providers.get(pipeline_id).map(|id| *id) {
            return Ok(provider_id);
        }
        let provider_id = self
            .repository
            .get_cached_pipelines(None)
            .await?
            .into_iter()
            .find(|pipeline| pipeline.id == pipeline_id)
            .map(|pipeline| pipeline.provider_id)
            .ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))?;
        self.pipeline_providers
            .insert(pipeline_id.to_string(), provider_id);

        Ok(provider_id)
    }

    async fn lease_of(&self, pipeline_id: &str) -> DomainResult<(ProviderLease, CacheEpoch)> {
        let provider_id = self.provider_of(pipeline_id).await?;
        let lease = match self.provider_service.acquire(provider_id).await {
            Err(DomainError::ProviderNotFound(_)) => {
                self.pipeline_providers.remove(pipeline_id);
                return Err(DomainError::PipelineNotFound(pipeline_id.to_string()));
            }
            other => other?,
        };
        let epoch = self.store.epoch(provider_id);
        if !lease.provider.capabilities().logs {
            return Err(DomainError::NotSupported(
                "Run logs are not available for this provider".to_string(),
            ));
        }

        Ok((lease, epoch))
    }

    async fn doc_key(
        &self, pipeline_id: &str, run_number: i64, node_id: &str,
    ) -> DomainResult<DocKey> {
        Ok(DocKey {
            provider_id: self.provider_of(pipeline_id).await?,
            pipeline_id: pipeline_id.to_string(),
            run_number,
            node_id: node_id.to_string(),
        })
    }

    pub async fn run_log_tree(
        self: &Arc<Self>, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<RunLogTree> {
        let (lease, epoch) = self.lease_of(pipeline_id).await?;
        let built = self.tree_of(&lease, epoch, pipeline_id, run_number).await?;
        if !built.0.complete {
            let run = (pipeline_id.to_string(), run_number);
            self.watch(&lease, epoch, &run, None, &built)?;
        }

        Ok(RunLogTree::clone(&built.0))
    }

    async fn tree_of(
        &self, lease: &ProviderLease, epoch: CacheEpoch, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Built> {
        let request = hash_request(
            lease.id,
            lease.generation,
            &format!(
                "run-log-tree:{}:{}:{pipeline_id}:{run_number}",
                epoch.global, epoch.provider
            ),
        );
        let built = self
            .trees
            .deduplicate(request, || {
                self.build_tree(lease, epoch, pipeline_id, run_number)
            })
            .await?;
        if self.store.epoch(lease.id) == epoch {
            self.memoize(
                (
                    lease.id,
                    lease.generation,
                    epoch,
                    pipeline_id.to_string(),
                    run_number,
                ),
                built.clone(),
            );
        }

        Ok(built)
    }

    async fn build_tree(
        &self, lease: &ProviderLease, epoch: CacheEpoch, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Built> {
        let run = self
            .provider_call(
                lease,
                lease.provider.fetch_run_details(pipeline_id, run_number),
            )
            .await?;
        if run.status.is_terminal() {
            if let Some(stored) = self
                .store
                .read_tree(lease.id, pipeline_id, run_number)
                .await
            {
                if stored.run_status == run.status && stored.run_concluded_at == run.concluded_at {
                    return Ok((Arc::new(stored.tree), Arc::new(run)));
                }
            }
        }

        let nodes = self
            .provider_call(lease, lease.provider.fetch_log_nodes(&run))
            .await?;
        let tree = tree_from_nodes(&run, nodes);
        if tree.complete {
            let stored = StoredTree {
                run_status: run.status.clone(),
                run_concluded_at: run.concluded_at,
                tree: tree.clone(),
            };
            let saved = self
                .provider_service
                .commit(lease, || {
                    self.store
                        .write_tree(lease.id, pipeline_id, run_number, &stored, epoch)
                })
                .await;
            if let Some(Err(e)) = saved {
                tracing::warn!(provider_id = lease.id, error = %e, "Failed to save the run log tree");
            }
        }

        Ok((Arc::new(tree), Arc::new(run)))
    }

    fn memoize(&self, key: TreeKey, built: Built) {
        let now = Instant::now();
        self.tree_memo
            .retain(|_, (at, _, _)| now.saturating_duration_since(*at) < COMPLETE_TREE_TTL);
        self.tree_memo.insert(key, (now, built.0, built.1));
    }

    fn memoized(
        &self, lease: &ProviderLease, epoch: CacheEpoch, pipeline_id: &str, run_number: i64,
    ) -> Option<Built> {
        let entry = self.tree_memo.get(&(
            lease.id,
            lease.generation,
            epoch,
            pipeline_id.to_string(),
            run_number,
        ))?;
        let (at, tree, run) = entry.value();
        let ttl = if tree.complete {
            COMPLETE_TREE_TTL
        } else {
            OPEN_TREE_TTL
        };

        (Instant::now().saturating_duration_since(*at) < ttl)
            .then(|| (Arc::clone(tree), Arc::clone(run)))
    }

    fn ensure_active(&self, provider_id: i64) -> DomainResult<()> {
        let now = Utc::now();
        match self
            .provider_service
            .scheduler()
            .paused_until(provider_id, now)
        {
            Some(until) => Err(DomainError::RateLimited {
                message: "Provider API quota or rate-limit cooldown is active".to_string(),
                retry_after: (until - now).to_std().ok(),
            }),
            None => Ok(()),
        }
    }

    async fn note_limit(&self, lease: &ProviderLease, error: &DomainError) {
        if matches!(
            error,
            DomainError::RateLimited { .. } | DomainError::ServiceUnavailable { .. }
        ) {
            self.provider_service.check_quota(lease).await;
        }
    }

    async fn provider_call<T>(
        &self, lease: &ProviderLease, call: impl Future<Output = DomainResult<T>>,
    ) -> DomainResult<T> {
        self.ensure_active(lease.id)?;
        let result = call.await;
        if let Err(e) = &result {
            self.note_limit(lease, e).await;
        }

        result
    }

    pub async fn open_document(
        self: &Arc<Self>, pipeline_id: &str, run_number: i64, node_id: &str, retry: bool,
    ) -> DomainResult<RunLogDocument> {
        let (lease, epoch) = self.lease_of(pipeline_id).await?;
        let built = match self.memoized(&lease, epoch, pipeline_id, run_number) {
            Some(built) => built,
            None => self.tree_of(&lease, epoch, pipeline_id, run_number).await?,
        };
        let (tree, run) = &built;
        let node = tree
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .ok_or_else(|| {
                DomainError::NotFound(format!("Job {node_id} isn't part of run #{run_number}"))
            })?;
        let Some(availability) = node.log else {
            return Err(DomainError::NotFound(format!(
                "{} has no log of its own",
                node.name
            )));
        };
        let run_key = (pipeline_id.to_string(), run_number);
        if availability == LogAvailability::Pending {
            self.watch(&lease, epoch, &run_key, Some(node_id), &built)?;
            return Ok(RunLogDocument::Pending);
        }

        let key = DocKey {
            provider_id: lease.id,
            pipeline_id: pipeline_id.to_string(),
            run_number,
            node_id: node_id.to_string(),
        };
        let failure = self.recent_failure(&key, &lease, epoch, retry);
        let meta = self.stored(&key, StoredDocMeta::to_meta).await?;
        let live = availability != LogAvailability::Ready
            || meta.as_ref().is_some_and(|meta| !meta.complete);
        if live {
            self.watch(&lease, epoch, &run_key, Some(node_id), &built)?;
        }
        if let Some(meta) = meta {
            return Ok(RunLogDocument::Ready(meta));
        }
        if let Some(received_bytes) = self.running(&key, &lease, epoch) {
            return Ok(RunLogDocument::Loading { received_bytes });
        }
        if let Some(message) = failure {
            return Ok(RunLogDocument::Unavailable { message });
        }

        Ok(RunLogDocument::Loading {
            received_bytes: self.claim(&key, lease, epoch, node, tree, Arc::clone(run))?,
        })
    }

    async fn stored<T>(
        &self, key: &DocKey, read: impl FnOnce(&StoredDocMeta) -> T,
    ) -> DomainResult<Option<T>> {
        let Some(revision) = self.store.current(key) else {
            return Ok(None);
        };

        Ok(self
            .store
            .lease(key, revision)
            .await?
            .map(|(doc, _lease)| read(doc.meta())))
    }

    fn recent_failure(
        &self, key: &DocKey, lease: &ProviderLease, epoch: CacheEpoch, retry: bool,
    ) -> Option<String> {
        let record = self.attempts.get(key)?;
        if retry || record.generation != lease.generation || record.epoch != epoch {
            return None;
        }
        let age = Instant::now().saturating_duration_since(record.at);

        record.failure.clone().filter(|_| age < FAILURE_COOLDOWN)
    }

    fn running(&self, key: &DocKey, lease: &ProviderLease, epoch: CacheEpoch) -> Option<u64> {
        let attempt = self.inflight.get(key)?;

        (attempt.generation == lease.generation && attempt.epoch == epoch)
            .then(|| attempt.received.load(Ordering::Relaxed))
    }

    fn claim(
        self: &Arc<Self>, key: &DocKey, lease: ProviderLease, epoch: CacheEpoch, node: &RunLogNode,
        tree: &RunLogTree, run: Arc<PipelineRun>,
    ) -> DomainResult<u64> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = self.root.child_token();
        let received = Arc::new(AtomicU64::new(0));
        let attempt = Attempt {
            id,
            generation: lease.generation,
            epoch,
            received: Arc::clone(&received),
            cancel: cancel.clone(),
        };

        {
            match self.inflight.entry(key.clone()) {
                Entry::Occupied(mut entry) => {
                    let current = entry.get();
                    if current.generation == lease.generation && current.epoch == epoch {
                        return Ok(current.received.load(Ordering::Relaxed));
                    }
                    current.cancel.cancel();
                    entry.insert(attempt);
                }
                Entry::Vacant(entry) => {
                    entry.insert(attempt);
                }
            }
        }

        let guard = AttemptGuard {
            service: Arc::clone(self),
            key: key.clone(),
            id,
        };
        let work = IngestWork {
            service: Arc::clone(self),
            lease,
            epoch,
            key: key.clone(),
            node: node.clone(),
            run,
            steps: step_bounds(node, tree),
            revision: self.store.allocate_revision(),
            cancel,
            received,
        };
        self.admit(move |tracker| {
            tracker.spawn(ingest(guard, work));
        })?;

        Ok(0)
    }

    fn interrupted(
        &self, provider_id: i64, epoch: CacheEpoch, cancel: &CancellationToken,
    ) -> Option<Stop> {
        let now = self.store.epoch(provider_id);
        if self.root.is_cancelled() || now.global != epoch.global {
            Some(Stop::Silent)
        } else if now.provider != epoch.provider {
            Some(Stop::ProviderChanged)
        } else if cancel.is_cancelled() {
            Some(Stop::Silent)
        } else {
            None
        }
    }

    fn busy_runs(&self) -> HashSet<(i64, String)> {
        self.inflight
            .iter()
            .map(|entry| entry.key().run_id())
            .collect()
    }

    async fn conclude(
        &self, guard: AttemptGuard, finished: Finished, generation: Generation, epoch: CacheEpoch,
        partial: bool,
    ) {
        let key = guard.key.clone();
        let record = |failure: Option<String>, resumable: bool| AttemptRecord {
            generation,
            epoch,
            at: Instant::now(),
            failure,
            resumable,
        };
        let failed = |message: String| CoreEvent::RunLogFailed {
            pipeline_id: key.pipeline_id.clone(),
            run_number: key.run_number,
            node_id: key.node_id.clone(),
            message,
        };
        let event = match finished {
            Finished::Stopped(Stop::Silent) => None,
            Finished::Stopped(Stop::ProviderChanged) => Some(failed(PROVIDER_CHANGED.to_string())),
            Finished::Published {
                revision,
                resumable,
            } => {
                if partial {
                    self.attempts.insert(key.clone(), record(None, resumable));
                } else {
                    self.attempts.remove(&key);
                }
                Some(CoreEvent::RunLogReady {
                    pipeline_id: key.pipeline_id.clone(),
                    run_number: key.run_number,
                    node_id: key.node_id.clone(),
                    revision,
                })
            }
            Finished::Unchanged { resumable } => {
                self.attempts.insert(key.clone(), record(None, resumable));
                None
            }
            Finished::Failed(message) => {
                self.attempts
                    .insert(key.clone(), record(Some(message.clone()), false));
                Some(failed(message))
            }
        };
        drop(guard);
        self.wake(&key);
        if let Some(event) = event {
            self.event_bus.emit(event).await;
        }
    }

    async fn leased(
        &self, pipeline_id: &str, run_number: i64, node_id: &str, revision: i64,
    ) -> DomainResult<(DocKey, Arc<Document>, ReadLease)> {
        let key = self.doc_key(pipeline_id, run_number, node_id).await?;
        match self.store.lease(&key, revision).await? {
            Some((doc, lease)) => Ok((key, doc, lease)),
            None => Err(DomainError::NotFound(REVISION_GONE.to_string())),
        }
    }

    fn doc_failure(&self, key: &DocKey, revision: i64, error: DocError) -> DomainError {
        match error {
            DocError::Corrupt => {
                self.store.mark_corrupt(key, revision);
                DomainError::InternalError("Log cache is corrupted".to_string())
            }
            DocError::Io(e) => DomainError::InternalError(format!("Couldn't read the log: {e}")),
            DocError::Cancelled => DomainError::InternalError("Log read cancelled".to_string()),
        }
    }

    async fn sweep(&self, reading: &DocKey) {
        if let Err(e) = self.store.sweep_with(self.busy_runs(), Some(reading)).await {
            tracing::warn!(error = %e, "Failed to remove stale run logs");
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn read_lines(
        &self, pipeline_id: &str, run_number: i64, node_id: &str, revision: i64, start: u32,
        count: u32, max_line_units: u32,
    ) -> DomainResult<LogLinesPage> {
        let (key, doc, lease) = self
            .leased(pipeline_id, run_number, node_id, revision)
            .await?;
        let store = Arc::clone(&self.store);
        let page = tokio::task::spawn_blocking(move || {
            let page = doc.read_lines(store.block_cache(), start, count, max_line_units);
            drop(lease);
            page
        })
        .await
        .map_err(|e| DomainError::InternalError(format!("Log reader stopped: {e}")))?;
        self.sweep(&key).await;

        page.map_err(|e| self.doc_failure(&key, revision, e))
    }

    pub async fn search(
        &self, pipeline_id: &str, run_number: i64, node_id: &str, revision: i64, session_id: &str,
        query: &LogSearchQuery,
    ) -> DomainResult<LogSearchResult> {
        let Some(regex) = search_regex(query)? else {
            return Ok(LogSearchResult::default());
        };

        let (key, doc, lease) = self
            .leased(pipeline_id, run_number, node_id, revision)
            .await?;

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = self.root.child_token();
        let session_key = (key.clone(), session_id.to_string());
        if let Some((_, previous)) = self
            .searches
            .insert(session_key.clone(), (id, cancel.clone()))
        {
            previous.cancel();
        }
        let _session = SearchSession {
            searches: &self.searches,
            key: session_key,
            id,
            cancel: cancel.clone(),
        };

        let permit = tokio::select! {
            permit = Arc::clone(&self.search_permits).acquire_owned() => permit,
            _ = cancel.cancelled() => return Err(search_cancelled()),
        }
        .map_err(|_| search_cancelled())?;

        let (tx, rx) = oneshot::channel();
        self.admit(|tracker| {
            tracker.spawn_blocking(move || {
                let result = doc.search(&regex, &cancel);
                drop(lease);
                drop(permit);
                let _ = tx.send(result);
            });
        })?;
        let result = rx.await.map_err(|_| search_cancelled())?;
        self.sweep(&key).await;

        result.map_err(|e| match e {
            DocError::Cancelled => search_cancelled(),
            other => self.doc_failure(&key, revision, other),
        })
    }

    pub async fn prepare_export(
        self: &Arc<Self>, pipeline_id: &str, run_number: i64, node_id: &str, revision: i64,
    ) -> DomainResult<LogExport> {
        let provider_id = self.provider_of(pipeline_id).await?;
        let epoch = self.store.epoch(provider_id);
        let (_, doc, lease) = self
            .leased(pipeline_id, run_number, node_id, revision)
            .await?;

        Ok(LogExport {
            doc,
            lease,
            provider_id,
            epoch,
            service: Arc::clone(self),
        })
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

impl LogExport {
    pub fn into_stream(
        self,
    ) -> DomainResult<impl Stream<Item = io::Result<Bytes>> + Send + 'static> {
        let Self {
            doc,
            lease,
            provider_id,
            epoch,
            service,
        } = self;
        if service.store.epoch(provider_id) != epoch {
            return Err(DomainError::NotFound(REVISION_GONE.to_string()));
        }

        let (tx, rx) = mpsc::channel::<io::Result<Bytes>>(EXPORT_QUEUE);
        let finished = Arc::new(AtomicBool::new(false));
        let root = service.root.clone();
        let done = Arc::clone(&finished);
        service.admit(|tracker| {
            tracker.spawn(export(doc, lease, tx, root, done));
        })?;

        Ok(futures::stream::unfold(Some(rx), move |rx| {
            let finished = Arc::clone(&finished);
            async move {
                let mut rx = rx?;
                match rx.recv().await {
                    Some(item) => Some((item, Some(rx))),
                    None if finished.load(Ordering::SeqCst) => None,
                    None => Some((Err(io::Error::other(EXPORT_INTERRUPTED)), None)),
                }
            }
        }))
    }
}

async fn export(
    doc: Arc<Document>, lease: ReadLease, tx: mpsc::Sender<io::Result<Bytes>>,
    root: CancellationToken, finished: Arc<AtomicBool>,
) {
    let _lease = lease;
    for index in 0..doc.block_count() {
        if root.is_cancelled() {
            return;
        }
        let block = Arc::clone(&doc);
        let item = match tokio::task::spawn_blocking(move || block.text_block(index)).await {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(e)) => Err(io::Error::other(e.to_string())),
            Err(e) => Err(io::Error::other(e)),
        };
        let failed = item.is_err();
        tokio::select! {
            sent = tx.send(item) => {
                if sent.is_err() {
                    return;
                }
            }
            _ = root.cancelled() => return,
        }
        if failed {
            finished.store(true, Ordering::SeqCst);
            return;
        }
    }
    finished.store(true, Ordering::SeqCst);
}

async fn ingest(guard: AttemptGuard, work: IngestWork) {
    let service = Arc::clone(&work.service);
    let generation = work.lease.generation;
    let epoch = work.epoch;
    let partial = work.node.log == Some(LogAvailability::Partial);
    let finished = work.run().await;
    service
        .conclude(guard, finished, generation, epoch, partial)
        .await;
}

impl IngestWork {
    async fn run(&self) -> Finished {
        let service = &self.service;
        let provider_id = self.key.provider_id;
        let interrupted = || service.interrupted(provider_id, self.epoch, &self.cancel);

        let _permit = tokio::select! {
            permit = Arc::clone(&service.ingest_permits).acquire_owned() => permit,
            _ = self.cancel.cancelled() => {
                return Finished::Stopped(interrupted().unwrap_or(Stop::Silent));
            }
        };
        if let Some(stop) = interrupted() {
            return Finished::Stopped(stop);
        }

        if let Err(e) = service.ensure_active(provider_id) {
            return Finished::Failed(failure_message(&e));
        }

        let base = match service.store.base(&self.key).await {
            Ok(base) => base,
            Err(e) => return Finished::Failed(e.to_string()),
        };
        let tmp = match service.store.tmp_dir().await {
            Ok(tmp) => tmp,
            Err(e) => return Finished::Failed(e.to_string()),
        };
        let finished = self.transfer(&tmp, base).await;
        if !matches!(finished, Finished::Published { .. }) {
            service.store.discard(tmp).await;
        }

        finished
    }

    async fn transfer(&self, tmp: &Path, base: Option<Base>) -> Finished {
        let service = &self.service;
        let key = &self.key;
        let provider_id = key.provider_id;
        let interrupted = || service.interrupted(provider_id, self.epoch, &self.cancel);
        let expected = base.as_ref().map(|base| base.revision);
        let from = base
            .as_ref()
            .and_then(|base| base.cursor(&self.steps))
            .cloned();

        let (tx, mut rx) = mpsc::channel::<Bytes>(INGEST_QUEUE);
        let mut sink = LogSink::new(tx);
        let ingester = Ingester {
            key: key.clone(),
            revision: self.revision,
            complete: self.node.log == Some(LogAvailability::Ready),
            dir: tmp.to_path_buf(),
            dialect: self.lease.provider.log_dialect(),
            steps: self.steps.clone(),
            received: Arc::clone(&self.received),
            limits: BlockLimits::default(),
            base,
            restart: sink.restart_signal(),
        };
        let writer = tokio::task::spawn_blocking(move || ingester.run(&mut rx));

        let read_result = {
            let read = async {
                let result = self
                    .lease
                    .provider
                    .read_log(&self.run, &key.node_id, from.as_ref(), &mut sink)
                    .await;
                drop(sink);
                result
            };
            tokio::pin!(read);

            let mut ticker = tokio::time::interval(PROGRESS_TICK);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut reported = 0;
            loop {
                tokio::select! {
                    result = &mut read => break Some(result),
                    _ = self.cancel.cancelled() => break None,
                    _ = ticker.tick() => {
                        let total = self.received.load(Ordering::Relaxed);
                        if total != reported {
                            reported = total;
                            service
                                .event_bus
                                .emit(CoreEvent::RunLogProgress {
                                    pipeline_id: key.pipeline_id.clone(),
                                    run_number: key.run_number,
                                    node_id: key.node_id.clone(),
                                    received_bytes: total,
                                })
                                .await;
                        }
                        if service.store.epoch(provider_id) != self.epoch {
                            self.cancel.cancel();
                        }
                    }
                }
            }
        };

        let written = writer.await;
        if let Some(stop) = interrupted() {
            return Finished::Stopped(stop);
        }
        let ingested = match written {
            Ok(Ok(ingested)) => ingested,
            Ok(Err(e)) => return Finished::Failed(format!("Couldn't save the log: {e}")),
            Err(e) => return Finished::Failed(format!("Couldn't save the log: {e}")),
        };
        let read = match read_result {
            Some(Ok(read)) => Some(read),
            Some(Err(e)) if !ingested.truncated() => {
                service.note_limit(&self.lease, &e).await;
                return Finished::Failed(failure_message(&e));
            }
            _ => None,
        };
        if let Some(read) = &read {
            if ingested.resumed() != (read.mode == LogReadMode::Append) {
                return Finished::Failed(PROVIDER_MISBEHAVED.to_string());
            }
            if ingested.unchanged(read) {
                return Finished::Unchanged {
                    resumable: read.cursor.is_some(),
                };
            }
        }

        let cursor = read.and_then(|read| read.cursor);
        let outcome = match tokio::task::spawn_blocking(move || ingested.finish(cursor)).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(e)) => return Finished::Failed(format!("Couldn't save the log: {e}")),
            Err(e) => return Finished::Failed(format!("Couldn't save the log: {e}")),
        };

        let busy = service.busy_runs();
        let committed = service
            .provider_service
            .commit(&self.lease, || {
                service
                    .store
                    .publish(key, tmp, self.revision, self.epoch, expected, busy)
            })
            .await;
        match committed {
            Some(Ok(PublishOutcome::Published)) => Finished::Published {
                revision: self.revision,
                resumable: outcome.resumable,
            },
            Some(Ok(PublishOutcome::StaleEpoch)) => {
                Finished::Stopped(interrupted().unwrap_or(Stop::ProviderChanged))
            }
            Some(Ok(PublishOutcome::StaleBase)) => Finished::Unchanged { resumable: false },
            Some(Err(e)) => Finished::Failed(e.to_string()),
            None => Finished::Stopped(Stop::ProviderChanged),
        }
    }
}

fn search_cancelled() -> DomainError {
    DomainError::InternalError("Search cancelled".to_string())
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

fn step_bounds(node: &RunLogNode, tree: &RunLogTree) -> Vec<StepBound> {
    tree.nodes
        .iter()
        .filter(|child| {
            child.kind == LogNodeKind::Step
                && child.log.is_none()
                && child.parent_id.as_deref() == Some(node.id.as_str())
        })
        .map(|step| StepBound {
            node_id: step.id.clone(),
            title: step.name.clone(),
            status: step.status.clone(),
            started_ms: step.started_at.map(|at| at.timestamp_millis()),
            finished_ms: step.finished_at.map(|at| at.timestamp_millis()),
        })
        .collect()
}

fn tree_from_nodes(run: &PipelineRun, nodes: Vec<LogNode>) -> RunLogTree {
    let mut tree: Vec<RunLogNode> = Vec::with_capacity(nodes.len());
    let mut index = HashMap::with_capacity(nodes.len());
    for node in nodes {
        if index.contains_key(&node.id) {
            tracing::warn!(
                run_number = run.run_number,
                "Duplicate run log node ID ignored"
            );
            continue;
        }
        let parent = node
            .parent_id
            .as_ref()
            .and_then(|parent| index.get(parent).copied());
        let depth = parent.map_or(0, |parent: usize| tree[parent].depth.saturating_add(1));
        let duration_seconds = match (node.started_at, node.finished_at) {
            (Some(started), Some(finished)) => Some((finished - started).num_seconds()),
            _ => None,
        };
        index.insert(node.id.clone(), tree.len());
        tree.push(RunLogNode {
            navigable: node.log.is_some(),
            parent_id: parent.map(|_| node.parent_id.clone()).unwrap_or_default(),
            id: node.id,
            kind: node.kind,
            name: node.name,
            status: node.status,
            started_at: node.started_at,
            finished_at: node.finished_at,
            duration_seconds,
            log: node.log,
            web_url: node.web_url,
            depth,
        });
    }

    for position in (0..tree.len()).rev() {
        if !tree[position].navigable {
            continue;
        }
        let parent = tree[position]
            .parent_id
            .as_ref()
            .and_then(|parent| index.get(parent).copied());
        if let Some(parent) = parent {
            tree[parent].navigable = true;
        }
    }

    let complete = run.status.is_terminal()
        && tree.iter().all(|node| {
            node.status.is_terminal()
                && !matches!(
                    node.log,
                    Some(LogAvailability::Partial | LogAvailability::Pending)
                )
        });
    let with_log = || tree.iter().filter(|node| node.log.is_some());
    let default_node_id = with_log()
        .find(|node| node.status == PipelineStatus::Failed)
        .or_else(|| {
            with_log().find(|node| {
                matches!(
                    node.log,
                    Some(LogAvailability::Ready | LogAvailability::Partial)
                )
            })
        })
        .or_else(|| with_log().next())
        .map(|node| node.id.clone());

    RunLogTree {
        nodes: tree,
        default_node_id,
        complete,
    }
}

mod live;

#[cfg(test)]
mod tests;
