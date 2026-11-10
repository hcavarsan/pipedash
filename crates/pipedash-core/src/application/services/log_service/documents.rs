use std::collections::HashSet;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::Arc;

use dashmap::Entry;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::ingest::ingest;
use super::{
    Attempt,
    AttemptGuard,
    AttemptRecord,
    Finished,
    IngestWork,
    LogService,
    Stop,
    FAILURE_COOLDOWN,
    PROVIDER_CHANGED,
};
use crate::application::services::provider_service::ProviderLease;
use crate::application::Generation;
use crate::domain::{
    DomainError,
    DomainResult,
    LogAvailability,
    LogNodeKind,
    PipelineRun,
    RunLogDocument,
    RunLogNode,
    RunLogTree,
};
use crate::event::CoreEvent;
use crate::infrastructure::run_logs::{
    CacheEpoch,
    DocKey,
    StepBound,
    StoredDocMeta,
};

impl LogService {
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
        if retry {
            self.attempts.remove_if(&key, |_, record| {
                record.generation == lease.generation
                    && record.epoch == epoch
                    && record.failure.is_some()
            });
        }
        let failure = self.recent_failure(&key, &lease, epoch);
        let meta = self.stored(&key, StoredDocMeta::to_meta).await?;
        let live = availability != LogAvailability::Ready
            || meta.as_ref().is_some_and(|meta| !meta.complete);
        if live {
            self.watch(&lease, epoch, &run_key, Some(node_id), &built)?;
            if retry {
                self.wake(&key);
            }
        }
        if let Some(meta) = meta {
            return Ok(self.presenter.run_log_document(meta, failure));
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

    pub(super) async fn stored<T>(
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
        &self, key: &DocKey, lease: &ProviderLease, epoch: CacheEpoch,
    ) -> Option<String> {
        let record = self.attempts.get(key)?;
        if record.generation != lease.generation || record.epoch != epoch {
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

    pub(super) fn claim(
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

    pub(super) fn interrupted(
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

    pub(super) fn busy_runs(&self) -> HashSet<(i64, String)> {
        self.inflight
            .iter()
            .map(|entry| entry.key().run_id())
            .collect()
    }

    pub(super) async fn conclude(
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
