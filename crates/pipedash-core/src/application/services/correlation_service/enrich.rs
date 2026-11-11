mod commits;
mod lineage;
mod logs;
mod provenance;
mod registry;
#[cfg(test)]
mod tests;

use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};
use std::time::Duration;

use tokio::time::Instant;

use self::commits::{
    Cached,
    Lru,
    CAPACITY,
};
use super::vocab::REL_CONSUMED;
use super::{
    CorrelationService,
    HISTORY_BACKFILL_LIMIT,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ArtifactRow,
    PIPELINE_LEVEL,
};

const RETRY_AFTER: Duration = Duration::from_secs(600);

struct Pending {
    artifact: ArtifactRow,
    consumers: Vec<String>,
}

pub(super) struct EnrichState {
    cache: Mutex<Lru>,
    deferred: Mutex<HashMap<String, Instant>>,
    queued: AtomicBool,
    asked: Mutex<HashSet<String>>,
    backfills: Mutex<HashMap<String, bool>>,
}

impl Default for EnrichState {
    fn default() -> Self {
        Self {
            cache: Mutex::new(Lru::new(CAPACITY)),
            deferred: Mutex::new(HashMap::new()),
            queued: AtomicBool::new(false),
            asked: Mutex::new(HashSet::new()),
            backfills: Mutex::new(HashMap::new()),
        }
    }
}

impl EnrichState {
    fn recall(&self, key: &str) -> Option<Cached> {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
    }

    fn remember(&self, key: String, value: Cached) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put(key, value);
    }

    pub(super) fn forget_provider(&self, owns: impl Fn(&str) -> bool) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.backfills
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|pipeline, _| !owns(pipeline));
    }

    fn due(&self, key: &str) -> bool {
        self.deferred
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .is_none_or(|until| *until <= Instant::now())
    }

    fn defer(&self, key: String) {
        let mut deferred = self.deferred.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        deferred.retain(|_, until| *until > now);
        deferred.insert(key, now + RETRY_AFTER);
    }

    fn asked(&self, digest: &str) -> bool {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(digest)
    }

    fn mark_asked(&self, digest: &str) {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(digest.to_string());
    }

    fn backfill_state(&self, pipeline: &str) -> Option<bool> {
        self.backfills
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(pipeline)
            .copied()
    }

    fn claim_backfill(&self, pipeline: &str) -> bool {
        self.backfills
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(pipeline.to_string(), false)
            .is_none()
    }

    fn finish_backfill(&self, pipeline: &str) {
        self.backfills
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(pipeline.to_string(), true);
        self.deferred
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|key, _| !key.starts_with("logs|"));
    }
}

impl CorrelationService {
    pub(super) fn schedule_enrichment(&self, changed: &HashSet<String>) {
        if changed.is_empty() {
            return;
        }
        if self.enrich.queued.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(this) = self.arc() else {
            self.enrich.queued.store(false, Ordering::SeqCst);
            return;
        };
        self.spawn(async move { this.enrich().await });
    }

    pub(super) async fn enrich(&self) {
        let Ok(_permit) = Arc::clone(&self.enrich_permits).acquire_owned().await else {
            return;
        };
        self.enrich.queued.store(false, Ordering::SeqCst);
        let mut touched = HashSet::new();
        touched.extend(gathered("registry", self.read_registries().await));
        if let Err(error) = self.backfill_source_pipelines().await {
            tracing::warn!(%error, "Correlation could not queue history for build pipelines");
        }
        touched.extend(gathered("logs", self.scan_logs().await));
        touched.extend(gathered("lineage", self.follow_lineage().await));
        if let Err(error) = self.fill_authored_at().await {
            tracing::warn!(%error, "Correlation could not read commit dates");
        }
        self.resolve_enriched(&touched).await;
    }

    pub(super) fn backfill_runs(&self, pipelines: impl IntoIterator<Item = String>) {
        let Some(this) = self.arc() else {
            return;
        };
        for pipeline in pipelines {
            if !self.enrich.claim_backfill(&pipeline) {
                continue;
            }
            let this = Arc::clone(&this);
            self.spawn(async move {
                if let Ok(permit) = Arc::clone(&this.enrich_permits).acquire_owned().await {
                    if let Err(error) = this
                        .pipeline_service
                        .fetch_run_history(&pipeline, HISTORY_BACKFILL_LIMIT)
                        .await
                    {
                        tracing::warn!(%error, pipeline, "Run history backfill failed");
                    }
                    drop(permit);
                }
                this.enrich.finish_backfill(&pipeline);
                this.schedule_enrichment(&HashSet::from([pipeline]));
            });
        }
    }

    pub(super) async fn backfill_for_repos(&self, repos: &BTreeSet<String>) -> DomainResult<()> {
        let mut conn = self.store.conn();
        let mut subjects = BTreeSet::new();
        for repo in repos {
            for row in conn.list_source_observations_by_repo(repo).await? {
                if row.run_number == PIPELINE_LEVEL {
                    subjects.insert(row.subject_id);
                }
            }
        }
        let mut missing = Vec::new();
        for subject in subjects {
            if self.enrich.backfill_state(&subject).is_some() {
                continue;
            }
            let unit = conn
                .get_subject_identity(&subject)
                .await?
                .is_some_and(|identity| identity.unit_id.is_some());
            if !unit
                && conn
                    .list_run_evidence_by_pipeline(&subject)
                    .await?
                    .is_empty()
            {
                missing.push(subject);
            }
        }
        self.backfill_runs(missing);
        Ok(())
    }

    async fn backfill_source_pipelines(&self) -> DomainResult<()> {
        let mut repos = BTreeSet::new();
        for item in self.unproduced(|_| true).await? {
            repos.extend(self.candidate_repos(&item).await?);
        }
        self.backfill_for_repos(&repos).await
    }

    async fn resolve_enriched(&self, touched: &HashSet<String>) {
        if touched.is_empty() {
            return;
        }
        let resolved = {
            let _guard = self.write_lock.lock().await;
            self.resolve(touched).await
        };
        match resolved {
            Ok(true) => self.changed(),
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "Correlation could not resolve enriched evidence"),
        }
    }
}

fn gathered(step: &str, result: DomainResult<HashSet<String>>) -> HashSet<String> {
    result.unwrap_or_else(|error| {
        tracing::warn!(%error, step, "Correlation enrichment step failed");
        HashSet::new()
    })
}

impl CorrelationService {
    async fn unproduced(&self, keep: impl Fn(&ArtifactRow) -> bool) -> DomainResult<Vec<Pending>> {
        let mut conn = self.store.conn();
        let mut pending = Vec::new();
        for artifact in conn
            .list_unproduced_artifacts()
            .await?
            .into_iter()
            .filter(|artifact| keep(artifact))
        {
            let mut consumers: Vec<String> = conn
                .list_artifact_observations_by_digest(&artifact.digest)
                .await?
                .into_iter()
                .filter(|row| row.relation == REL_CONSUMED)
                .map(|row| row.subject_id)
                .collect();
            consumers.sort();
            consumers.dedup();
            pending.push(Pending {
                artifact,
                consumers,
            });
        }
        Ok(pending)
    }
}
