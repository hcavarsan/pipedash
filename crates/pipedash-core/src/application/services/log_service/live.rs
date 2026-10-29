use tokio::sync::Notify;
use tokio::time::sleep_until;

use super::*;

pub(super) type RunKey = (String, i64);

pub(super) struct Interest {
    run: Instant,
    nodes: HashMap<String, Instant>,
}

pub(super) struct Live {
    id: u64,
    generation: Generation,
    epoch: CacheEpoch,
    pub(super) cancel: CancellationToken,
    wake: Arc<Notify>,
}

struct LiveGuard {
    service: Arc<LogService>,
    key: RunKey,
    id: u64,
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        let removed = self
            .service
            .lives
            .remove_if(&self.key, |_, live| live.id == self.id);
        if removed.is_some() {
            self.service.interests.remove(&self.key);
        }
    }
}

enum Visit {
    Settled,
    Waiting,
    At(Instant),
}

struct Watch {
    service: Arc<LogService>,
    _guard: LiveGuard,
    run: RunKey,
    generation: Generation,
    epoch: CacheEpoch,
    cancel: CancellationToken,
    wake: Arc<Notify>,
    last: Built,
    tree_due: Instant,
}

impl LogService {
    pub(super) fn watch(
        self: &Arc<Self>, lease: &ProviderLease, epoch: CacheEpoch, run: &RunKey,
        node: Option<&str>, built: &Built,
    ) -> DomainResult<()> {
        let now = Instant::now();
        {
            let mut interest = self
                .interests
                .entry(run.clone())
                .or_insert_with(|| Interest {
                    run: now,
                    nodes: HashMap::new(),
                });
            interest.run = now;
            if let Some(node) = node {
                interest.nodes.insert(node.to_string(), now);
            }
        }
        if self
            .lives
            .get(run)
            .is_some_and(|live| live.generation == lease.generation && live.epoch == epoch)
        {
            return Ok(());
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = self.root.child_token();
        let wake = Arc::new(Notify::new());
        let live = Live {
            id,
            generation: lease.generation,
            epoch,
            cancel: cancel.clone(),
            wake: Arc::clone(&wake),
        };
        match self.lives.entry(run.clone()) {
            Entry::Occupied(mut entry) => {
                if entry.get().generation == lease.generation && entry.get().epoch == epoch {
                    return Ok(());
                }
                entry.get().cancel.cancel();
                entry.insert(live);
            }
            Entry::Vacant(entry) => {
                entry.insert(live);
            }
        }

        let watch = Watch {
            service: Arc::clone(self),
            _guard: LiveGuard {
                service: Arc::clone(self),
                key: run.clone(),
                id,
            },
            run: run.clone(),
            generation: lease.generation,
            epoch,
            cancel,
            wake,
            last: built.clone(),
            tree_due: now + TREE_INTERVAL,
        };
        self.admit(move |tracker| {
            tracker.spawn(watch.drive());
        })
    }

    pub(super) fn wake(&self, key: &DocKey) {
        if let Some(live) = self.lives.get(&(key.pipeline_id.clone(), key.run_number)) {
            live.wake.notify_one();
        }
    }

    fn interested(&self, run: &RunKey, now: Instant) -> Option<Vec<String>> {
        let mut interest = self.interests.get_mut(run)?;
        if now.saturating_duration_since(interest.run) >= INTEREST_TTL {
            return None;
        }
        interest
            .nodes
            .retain(|_, seen| now.saturating_duration_since(*seen) < INTEREST_TTL);

        Some(interest.nodes.keys().cloned().collect())
    }

    fn paused_until(&self, provider_id: i64) -> Option<Instant> {
        let now = Utc::now();
        let until = self
            .provider_service
            .scheduler()
            .paused_until(provider_id, now)?;

        Some(Instant::now() + (until - now).to_std().unwrap_or_default())
    }
}

impl Watch {
    async fn drive(mut self) {
        let cancel = self.cancel.clone();
        let wake = Arc::clone(&self.wake);
        loop {
            let next = tokio::select! {
                _ = cancel.cancelled() => return,
                next = self.step() => next,
            };
            let Some(next) = next else {
                return;
            };
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = wake.notified() => {}
                _ = sleep_until(next) => {}
            }
        }
    }

    async fn step(&mut self) -> Option<Instant> {
        let service = Arc::clone(&self.service);
        let nodes = service.interested(&self.run, Instant::now())?;
        let (lease, epoch) = service.lease_of(&self.run.0).await.ok()?;
        if lease.generation != self.generation || epoch != self.epoch {
            return None;
        }
        if let Some(until) = service.paused_until(lease.id) {
            return Some(until);
        }

        if Instant::now() >= self.tree_due {
            self.refresh(&lease, epoch).await;
            self.tree_due = Instant::now() + TREE_INTERVAL;
        }
        let mut next = self.tree_due;
        let mut open = false;
        for id in &nodes {
            match self.visit(&lease, epoch, id).await? {
                Visit::Settled => {}
                Visit::Waiting => open = true,
                Visit::At(at) => {
                    open = true;
                    next = next.min(at);
                }
            }
        }

        (open || !self.last.1.status.is_terminal()).then_some(next)
    }

    async fn refresh(&mut self, lease: &ProviderLease, epoch: CacheEpoch) {
        let (pipeline_id, run_number) = (&self.run.0, self.run.1);
        let built = match self
            .service
            .tree_of(lease, epoch, pipeline_id, run_number)
            .await
        {
            Ok(built) => built,
            Err(e) => {
                tracing::debug!(pipeline_id, run_number, error = %e, "Failed to refresh the run log tree");
                return;
            }
        };
        let tree_changed = built.0 != self.last.0;
        let run_changed = built.1 != self.last.1;
        self.last = built;
        if tree_changed {
            self.service
                .event_bus
                .emit(CoreEvent::RunLogTreeChanged {
                    pipeline_id: pipeline_id.clone(),
                    run_number,
                })
                .await;
        }
        if run_changed {
            self.service
                .event_bus
                .emit(CoreEvent::RunUpdated {
                    pipeline_id: pipeline_id.clone(),
                    run_number,
                    run: PipelineRun::clone(&self.last.1),
                })
                .await;
        }
    }

    async fn visit(&self, lease: &ProviderLease, epoch: CacheEpoch, id: &str) -> Option<Visit> {
        let (tree, run) = &self.last;
        let Some(node) = tree.nodes.iter().find(|node| node.id == id) else {
            return Some(Visit::Settled);
        };
        let availability = match node.log {
            None => return Some(Visit::Settled),
            Some(LogAvailability::Pending) if run.status.is_terminal() => {
                return Some(Visit::Settled)
            }
            Some(LogAvailability::Pending) => return Some(Visit::Waiting),
            Some(availability) => availability,
        };
        let service = &self.service;
        let key = DocKey {
            provider_id: lease.id,
            pipeline_id: self.run.0.clone(),
            run_number: self.run.1,
            node_id: id.to_string(),
        };
        if service.inflight.contains_key(&key) {
            return Some(Visit::Waiting);
        }
        let finished = service
            .stored(&key, |meta| meta.complete || meta.truncated)
            .await
            .ok()
            .flatten();
        if finished == Some(true) {
            return Some(Visit::Settled);
        }

        let now = Instant::now();
        let due = match service
            .attempts
            .get(&key)
            .filter(|record| record.generation == lease.generation && record.epoch == epoch)
        {
            None => now,
            Some(record) if record.failure.is_some() => record.at + FAILURE_COOLDOWN,
            Some(_) if availability == LogAvailability::Ready => now,
            Some(record) if record.resumable => record.at + RESUMABLE_INTERVAL,
            Some(record) => record.at + SNAPSHOT_INTERVAL,
        };
        if due > now {
            return Some(Visit::At(due));
        }
        if let Some(until) = service.paused_until(lease.id) {
            return Some(Visit::At(until));
        }
        service
            .claim(&key, lease.clone(), epoch, node, tree, Arc::clone(run))
            .ok()?;

        Some(Visit::Waiting)
    }
}
