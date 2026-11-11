use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};
use std::sync::LazyLock;

use chrono::Utc;
use pipedash_plugin_api::{
    ChangedFile,
    CommitSha,
    Digest,
    ImageRef,
};
use regex::Regex;

use super::super::vocab::{
    commit_node_id,
    ARTIFACT,
    COMMIT,
    CONFIG_LINEAGE,
    INTRODUCED,
    REL_CONSUMED,
};
use super::super::CorrelationService;
use super::commits::{
    stored_repo,
    Cached,
};
use crate::application::services::provider_service::ProviderLease;
use crate::domain::application::Confidence;
use crate::domain::DomainResult;
use crate::infrastructure::database::CorrelationEdgeRow;

static KEYED: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?:image|newTag|tag|repository|digest)\s*[:=]\s*["']?([^\s"']+)"#).ok()
});
static PINNED: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"[\w.\-/:]+@sha256:[0-9a-f]{64}").ok());
static TAGGED: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"[\w.\-]+(?:\.[\w\-]+)+/[\w.\-/]+:[\w.\-]+").ok());

struct Consumed {
    repository: String,
    subjects: Vec<String>,
}

pub(super) fn added_references(files: &[ChangedFile]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for patch in files.iter().filter_map(|file| file.patch.as_deref()) {
        for line in patch
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        {
            let line = &line[1..];
            if let Some(keyed) = KEYED.as_ref() {
                found.extend(
                    keyed
                        .captures_iter(line)
                        .filter_map(|capture| capture.get(1))
                        .map(|value| value.as_str().to_string()),
                );
            }
            for pattern in [PINNED.as_ref(), TAGGED.as_ref()].into_iter().flatten() {
                found.extend(pattern.find_iter(line).map(|hit| hit.as_str().to_string()));
            }
        }
    }
    found
}

impl CorrelationService {
    pub(super) async fn follow_lineage(&self) -> DomainResult<HashSet<String>> {
        let mut touched = HashSet::new();
        let mut commits: HashMap<(String, String), Vec<String>> = HashMap::new();
        for row in self.store.conn().list_unit_sources().await? {
            let Some(sha) = row.commit_sha else {
                continue;
            };
            commits
                .entry((row.repo, sha))
                .or_default()
                .push(row.subject_id);
        }
        let mut pending = Vec::new();
        for ((repo, sha), subjects) in commits {
            let key = format!("lineage|{repo}|{sha}");
            let node = commit_node_id(&repo, &sha);
            let known = self.enrich.recall(&key).is_some()
                || !self.enrich.due(&key)
                || self
                    .store
                    .conn()
                    .list_edges_from(COMMIT, &node)
                    .await?
                    .iter()
                    .any(|edge| edge.rule == CONFIG_LINEAGE);
            if !known {
                pending.push((repo, sha, subjects, key, node));
            }
        }
        if pending.is_empty() {
            return Ok(touched);
        }
        let consumed = self.consumed_digests().await?;
        pending.sort();
        for (repo, sha, subjects, key, node) in pending {
            let (Some(repo_id), Some(sha_id)) = (stored_repo(&repo), CommitSha::parse(&sha)) else {
                continue;
            };
            let Some((files, lease)) = self.changes_of(&repo_id, &sha_id).await else {
                self.enrich.defer(key);
                continue;
            };
            let mut complete = true;
            for reference in added_references(&files) {
                let (digests, credentials) = self.introduced(&reference, &consumed).await;
                for digest in digests {
                    let linked = self
                        .commit_guarded(&lease, credentials.as_ref(), || {
                            self.link_lineage(&node, &digest, &reference)
                        })
                        .await;
                    let Some(linked) = linked else {
                        complete = false;
                        continue;
                    };
                    linked?;
                    touched.extend(subjects.iter().cloned());
                    if let Some(found) = consumed.get(&digest) {
                        touched.extend(found.subjects.iter().cloned());
                    }
                }
            }
            if complete {
                self.enrich.remember(key, Cached::Seen);
            }
        }
        Ok(touched)
    }

    async fn consumed_digests(&self) -> DomainResult<HashMap<String, Consumed>> {
        let mut consumed: HashMap<String, Consumed> = HashMap::new();
        for row in self.store.conn().list_artifact_observations().await? {
            let (REL_CONSUMED, Some(digest)) = (row.relation.as_str(), row.digest) else {
                continue;
            };
            let entry = consumed.entry(digest).or_insert_with(|| Consumed {
                repository: row.repository,
                subjects: Vec::new(),
            });
            entry.subjects.push(row.subject_id);
        }
        Ok(consumed)
    }

    async fn introduced(
        &self, reference: &str, consumed: &HashMap<String, Consumed>,
    ) -> (Vec<String>, Option<ProviderLease>) {
        let pinned = |digest: &Digest| {
            consumed
                .contains_key(digest.as_str())
                .then(|| digest.as_str().to_string())
                .into_iter()
                .collect()
        };
        if let Some(digest) = Digest::parse(reference) {
            return (pinned(&digest), None);
        }
        let Some(image) = ImageRef::parse(reference) else {
            return (Vec::new(), None);
        };
        if let Some(digest) = image.digest {
            return (pinned(&digest), None);
        }
        let Some(tag) = image.tag else {
            return (Vec::new(), None);
        };
        let candidates: HashSet<&str> = consumed
            .iter()
            .filter(|(_, found)| found.repository == image.repository.as_str())
            .map(|(digest, _)| digest.as_str())
            .collect();
        if candidates.is_empty() || !self.config_state.get().await.correlation.registry_read {
            return (Vec::new(), None);
        }
        let Ok(client) = self.oci_client() else {
            return (Vec::new(), None);
        };
        let access = self.registry_access(image.repository.host()).await;
        let auth = access.as_ref().map(|(auth, _)| auth);
        let resolved = client.tag_digests(&image.repository, &tag, auth).await;
        let lease = access.map(|(_, lease)| lease);
        match resolved {
            Ok(digests) => (
                digests
                    .iter()
                    .map(Digest::as_str)
                    .filter(|digest| candidates.contains(digest))
                    .map(str::to_string)
                    .collect(),
                lease,
            ),
            Err(error) => {
                tracing::debug!(%error, "Tag in config could not be resolved");
                (Vec::new(), None)
            }
        }
    }

    async fn link_lineage(&self, node: &str, digest: &str, reference: &str) -> DomainResult<()> {
        let now = Utc::now();
        self.store
            .conn()
            .upsert_edge(&CorrelationEdgeRow {
                id: 0,
                from_kind: COMMIT.to_string(),
                from_id: node.to_string(),
                to_kind: ARTIFACT.to_string(),
                to_id: digest.to_string(),
                relation: INTRODUCED.to_string(),
                confidence: Confidence::Exact.as_str().to_string(),
                rule: CONFIG_LINEAGE.to_string(),
                evidence_json: serde_json::json!({ "reference": reference }).to_string(),
                first_seen: now,
                last_seen: now,
            })
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests;
