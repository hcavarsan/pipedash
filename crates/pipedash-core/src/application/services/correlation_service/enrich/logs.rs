use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
    HashSet,
};
use std::time::Duration;

use chrono::{
    Duration as Span,
    Utc,
};
use futures::StreamExt;
use pipedash_plugin_api::correlation::log_scan::pushed_digests;
use sha2::{
    Digest as _,
    Sha256,
};
use tokio::time::{
    sleep,
    Instant,
};

use super::super::vocab::{
    parse_commit_node,
    ARTIFACT,
    CONFIG_LINEAGE,
    ORIGIN_LOG,
    REL_PRODUCED,
};
use super::super::{
    CorrelationService,
    LOG_SCAN_HOURS,
    LOG_SCAN_RUNS,
};
use super::Pending;
use crate::domain::{
    DomainError,
    DomainResult,
    RunLogDocument,
};
use crate::infrastructure::database::{
    ArtifactObservationRow,
    CorrelationConn,
    LogScanRow,
    RunEvidenceRow,
};

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const READY_POLL: Duration = Duration::from_millis(100);
const MAX_LINE_BYTES: usize = 64 * 1024;

type Wanted = BTreeMap<String, String>;

struct Candidate {
    run: RunEvidenceRow,
    provider_id: i64,
    wanted: Wanted,
}

enum Opened {
    Ready(i64),
    Absent,
    Retry,
}

#[derive(Default)]
struct Lines {
    rest: Vec<u8>,
}

impl Lines {
    fn push(&mut self, chunk: &[u8], mut line: impl FnMut(&str)) {
        self.rest.extend_from_slice(chunk);
        while let Some(end) = self.rest.iter().position(|byte| *byte == b'\n') {
            let taken: Vec<u8> = self.rest.drain(..=end).collect();
            line(String::from_utf8_lossy(&taken).trim_end_matches(['\r', '\n']));
        }
        if self.rest.len() > MAX_LINE_BYTES {
            self.rest.clear();
        }
    }

    fn finish(&mut self, mut line: impl FnMut(&str)) {
        if !self.rest.is_empty() {
            line(&String::from_utf8_lossy(&self.rest));
            self.rest.clear();
        }
    }
}

pub(super) fn run_hash(run: &RunEvidenceRow, wanted: &Wanted) -> String {
    let digests: Vec<&str> = wanted.keys().map(String::as_str).collect();
    let state = format!(
        "{}|{}|{:?}|{}",
        run.status,
        run.started_at.to_rfc3339(),
        run.concluded_at.map(|at| at.to_rfc3339()),
        digests.join(",")
    );
    Sha256::digest(state.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn record(line: &str, wanted: &Wanted, found: &mut BTreeMap<String, String>) {
    let Some((repository, digest)) = pushed_digests(line) else {
        return;
    };
    let Some(own) = wanted.get(digest.as_str()) else {
        return;
    };
    let repository = repository.map_or_else(|| own.clone(), |r| r.as_str().to_string());
    found.insert(digest.as_str().to_string(), repository);
}

impl CorrelationService {
    pub(super) async fn scan_logs(&self) -> DomainResult<HashSet<String>> {
        if !self.config_state.get().await.correlation.log_evidence {
            return Ok(HashSet::new());
        }
        let pending = self
            .unproduced(|artifact| self.enrich.due(&format!("logs|{}", artifact.digest)))
            .await?;
        let mut runs: BTreeMap<(String, i64), Candidate> = BTreeMap::new();
        for item in &pending {
            for candidate in self.candidate_runs(item).await? {
                let key = (candidate.run.pipeline_id.clone(), candidate.run.run_number);
                runs.entry(key)
                    .or_insert_with(|| Candidate {
                        wanted: Wanted::new(),
                        ..candidate
                    })
                    .wanted
                    .insert(
                        item.artifact.digest.clone(),
                        item.artifact.repository.clone(),
                    );
            }
        }
        let consumers: HashMap<&str, &Vec<String>> = pending
            .iter()
            .map(|item| (item.artifact.digest.as_str(), &item.consumers))
            .collect();
        let mut touched = HashSet::new();
        for ((pipeline, run_number), candidate) in runs {
            let hash = run_hash(&candidate.run, &candidate.wanted);
            let scanned = self
                .store
                .conn()
                .get_log_scan(&pipeline, run_number)
                .await?;
            if scanned.is_some_and(|scan| scan.run_hash == hash) {
                continue;
            }
            let Some(found) = self
                .scan_run(&pipeline, run_number, &candidate.wanted)
                .await
            else {
                continue;
            };
            self.write_scan(&candidate, hash, &found).await?;
            if !found.is_empty() {
                touched.insert(pipeline);
            }
            for digest in found.keys() {
                touched.extend(
                    consumers
                        .get(digest.as_str())
                        .into_iter()
                        .flat_map(|c| c.iter().cloned()),
                );
            }
        }
        let now = Utc::now();
        for item in &pending {
            self.store
                .conn()
                .mark_artifact_log_scanned(&item.artifact.digest, now)
                .await?;
            self.enrich.defer(format!("logs|{}", item.artifact.digest));
        }
        Ok(touched)
    }

    pub(super) async fn candidate_repos(&self, item: &Pending) -> DomainResult<BTreeSet<String>> {
        let mut repos: BTreeSet<String> = item.artifact.label_source.iter().cloned().collect();
        for edge in self
            .store
            .conn()
            .list_edges_to(ARTIFACT, &item.artifact.digest)
            .await?
        {
            if edge.rule != CONFIG_LINEAGE {
                continue;
            }
            if let Some((repo, _)) = parse_commit_node(&edge.from_id) {
                repos.insert(repo.to_string());
            }
        }
        Ok(repos)
    }

    async fn candidate_runs(&self, item: &Pending) -> DomainResult<Vec<Candidate>> {
        let repos = self.candidate_repos(item).await?;
        if repos.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.store.conn();
        let mut providers: HashMap<String, i64> = HashMap::new();
        for repo in &repos {
            for row in conn.list_source_observations_by_repo(repo).await? {
                providers.insert(row.subject_id, row.provider_id);
            }
        }
        let first_seen = item.artifact.first_seen;
        let mut runs = conn
            .list_run_evidence_concluded_between(
                first_seen - Span::hours(LOG_SCAN_HOURS),
                first_seen,
            )
            .await?;
        runs.retain(|run| providers.contains_key(&run.pipeline_id));
        runs.sort_by_key(|run| std::cmp::Reverse(run.concluded_at));
        runs.truncate(LOG_SCAN_RUNS);
        Ok(runs
            .into_iter()
            .filter_map(|run| {
                let provider_id = *providers.get(&run.pipeline_id)?;
                Some(Candidate {
                    run,
                    provider_id,
                    wanted: Wanted::new(),
                })
            })
            .collect())
    }

    async fn scan_run(&self, pipeline: &str, run_number: i64, wanted: &Wanted) -> Option<Wanted> {
        let tree = self
            .log_service
            .run_log_tree(pipeline, run_number)
            .await
            .inspect_err(|error| tracing::debug!(%error, pipeline, "Log tree unavailable"))
            .ok()?;
        let mut found = Wanted::new();
        for node in tree.nodes.iter().filter(|node| node.log.is_some()) {
            match self.open_ready(pipeline, run_number, &node.id).await {
                Opened::Ready(revision) => {
                    if !self
                        .read_node(pipeline, run_number, &node.id, revision, wanted, &mut found)
                        .await
                    {
                        return None;
                    }
                }
                Opened::Absent => {}
                Opened::Retry => return None,
            }
        }
        Some(found)
    }

    async fn open_ready(&self, pipeline: &str, run_number: i64, node: &str) -> Opened {
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut retry = true;
        loop {
            match self
                .log_service
                .open_document(pipeline, run_number, node, retry)
                .await
            {
                Ok(RunLogDocument::Ready(meta)) => return Opened::Ready(meta.revision),
                Ok(RunLogDocument::Loading { .. }) => {}
                Ok(RunLogDocument::Pending | RunLogDocument::Unavailable { .. }) => {
                    return Opened::Retry;
                }
                Err(DomainError::NotFound(_)) => return Opened::Absent,
                Err(error) => {
                    tracing::debug!(%error, pipeline, "Log document unavailable");
                    return Opened::Retry;
                }
            }
            retry = false;
            if Instant::now() >= deadline {
                return Opened::Retry;
            }
            sleep(READY_POLL).await;
        }
    }

    async fn read_node(
        &self, pipeline: &str, run_number: i64, node: &str, revision: i64, wanted: &Wanted,
        found: &mut Wanted,
    ) -> bool {
        let stream = match self
            .log_service
            .prepare_export(pipeline, run_number, node, revision)
            .await
            .and_then(|export| export.into_stream())
        {
            Ok(stream) => stream,
            Err(error) => {
                tracing::debug!(%error, pipeline, "Log export unavailable");
                return false;
            }
        };
        futures::pin_mut!(stream);
        let mut lines = Lines::default();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => lines.push(&chunk, |line| record(line, wanted, found)),
                Err(error) => {
                    tracing::debug!(%error, pipeline, "Log read interrupted");
                    return false;
                }
            }
        }
        lines.finish(|line| record(line, wanted, found));
        true
    }

    async fn write_scan(
        &self, candidate: &Candidate, hash: String, found: &Wanted,
    ) -> DomainResult<()> {
        let pipeline = candidate.run.pipeline_id.clone();
        let run_number = candidate.run.run_number;
        let rows: Vec<ArtifactObservationRow> = found
            .iter()
            .map(|(digest, repository)| ArtifactObservationRow {
                provider_id: candidate.provider_id,
                subject_id: pipeline.clone(),
                run_number,
                relation: REL_PRODUCED.to_string(),
                ordinal: 0,
                repository: repository.clone(),
                digest: Some(digest.clone()),
                tag: None,
                target_ordinal: None,
                origin: ORIGIN_LOG.to_string(),
            })
            .collect();
        self.store
            .transaction(|conn: &mut CorrelationConn| {
                Box::pin(async move {
                    conn.replace_artifact_observations(&pipeline, run_number, ORIGIN_LOG, &rows)
                        .await?;
                    conn.upsert_log_scan(&LogScanRow {
                        pipeline_id: pipeline,
                        run_number,
                        run_hash: hash,
                        scanned_at: Utc::now(),
                    })
                    .await
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests;
