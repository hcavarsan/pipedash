use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
};

use chrono::{
    DateTime,
    Utc,
};
use serde_json::{
    json,
    Value,
};

use super::vocab::{
    commit_node_id,
    run_node_id,
    ARTIFACT,
    ATTESTATION,
    BUILT_FROM,
    COMMIT,
    DEPLOYS_TO,
    LOG_PUSH,
    ORIGIN_ATTESTATION,
    ORIGIN_LOG,
    PRODUCED,
    PROVIDER_PRODUCED,
    RUN,
    RUNS,
    RUN_SOURCE_COMMIT,
    SYNCED,
    TARGET,
    UNIT,
    UNIT_CONSUMED,
    UNIT_SOURCE_COMMIT,
    UNIT_TARGET,
};
use super::world::{
    Affected,
    Subject,
    World,
};
use super::CorrelationService;
use crate::domain::application::Confidence;
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ArtifactObservationRow,
    CorrelationConn,
    CorrelationEdgeRow,
    SourceObservationRow,
    PIPELINE_LEVEL,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub(super) struct EdgeSpec {
    pub from_kind: &'static str,
    pub from_id: String,
    pub to_kind: &'static str,
    pub to_id: String,
    pub relation: &'static str,
    pub confidence: Confidence,
    pub rule: &'static str,
    pub evidence: Value,
}

pub(super) type EdgeKey = (String, String, String, String, String, String);

pub(super) fn key_of_row(row: &CorrelationEdgeRow) -> EdgeKey {
    (
        row.from_kind.clone(),
        row.from_id.clone(),
        row.to_kind.clone(),
        row.to_id.clone(),
        row.relation.clone(),
        row.rule.clone(),
    )
}

impl EdgeSpec {
    pub(super) fn key(&self) -> EdgeKey {
        (
            self.from_kind.to_string(),
            self.from_id.clone(),
            self.to_kind.to_string(),
            self.to_id.clone(),
            self.relation.to_string(),
            self.rule.to_string(),
        )
    }

    fn row(&self, now: DateTime<Utc>) -> CorrelationEdgeRow {
        CorrelationEdgeRow {
            id: 0,
            from_kind: self.from_kind.to_string(),
            from_id: self.from_id.clone(),
            to_kind: self.to_kind.to_string(),
            to_id: self.to_id.clone(),
            relation: self.relation.to_string(),
            confidence: self.confidence.as_str().to_string(),
            rule: self.rule.to_string(),
            evidence_json: self.evidence.to_string(),
            first_seen: now,
            last_seen: now,
        }
    }
}

pub(super) struct Applied {
    pub changed: bool,
    pub ids: BTreeMap<EdgeKey, i64>,
}

pub(super) async fn apply(
    conn: &mut CorrelationConn, existing: Vec<CorrelationEdgeRow>, desired: &[EdgeSpec],
    deletable: impl Fn(&CorrelationEdgeRow) -> bool, now: DateTime<Utc>,
) -> DomainResult<Applied> {
    let mut stored: HashMap<EdgeKey, CorrelationEdgeRow> = existing
        .into_iter()
        .map(|row| (key_of_row(&row), row))
        .collect();
    let mut applied = Applied {
        changed: false,
        ids: BTreeMap::new(),
    };
    for spec in desired {
        let key = spec.key();
        let evidence = spec.evidence.to_string();
        let id = match stored.remove(&key) {
            Some(row)
                if row.confidence == spec.confidence.as_str() && row.evidence_json == evidence =>
            {
                row.id
            }
            _ => {
                applied.changed = true;
                conn.upsert_edge(&spec.row(now)).await?
            }
        };
        applied.ids.insert(key, id);
    }
    for row in stored.into_values().filter(|row| deletable(row)) {
        conn.delete_edge(row.id).await?;
        applied.changed = true;
    }
    Ok(applied)
}

fn run_specs(
    pipeline: &str, run: i64, sources: &[&SourceObservationRow],
    artifacts: &[&ArtifactObservationRow],
) -> Vec<EdgeSpec> {
    let node = run_node_id(pipeline, run);
    let mut specs = Vec::new();
    for source in sources {
        let Some(commit) = &source.commit_sha else {
            continue;
        };
        specs.push(EdgeSpec {
            from_kind: RUN,
            from_id: node.clone(),
            to_kind: COMMIT,
            to_id: commit_node_id(&source.repo, commit),
            relation: BUILT_FROM,
            confidence: Confidence::Exact,
            rule: RUN_SOURCE_COMMIT,
            evidence: json!({ "repo": source.repo, "commit": commit, "paths": source.path_scopes_json }),
        });
    }
    for artifact in artifacts {
        let Some(digest) = &artifact.digest else {
            continue;
        };
        let rule = match artifact.origin.as_str() {
            ORIGIN_ATTESTATION => ATTESTATION,
            ORIGIN_LOG => LOG_PUSH,
            _ => PROVIDER_PRODUCED,
        };
        specs.push(EdgeSpec {
            from_kind: RUN,
            from_id: node.clone(),
            to_kind: ARTIFACT,
            to_id: digest.clone(),
            relation: PRODUCED,
            confidence: Confidence::Exact,
            rule,
            evidence: json!({ "repository": artifact.repository, "digest": digest }),
        });
    }
    specs
}

pub(super) fn unit_specs(key: &str, subject: &Subject) -> Vec<EdgeSpec> {
    let mut specs = Vec::new();
    let mut seen: BTreeMap<&str, (BTreeSet<String>, &str, Option<&str>)> = BTreeMap::new();
    for consumed in &subject.consumed {
        let Some(digest) = &consumed.digest else {
            continue;
        };
        let entry = seen.entry(digest).or_insert_with(|| {
            (
                BTreeSet::new(),
                consumed.repository.as_str(),
                consumed.tag.as_deref(),
            )
        });
        if let Some(target) = consumed.target.and_then(|t| subject.targets.get(t)) {
            entry.0.insert(target.key());
        }
    }
    for (digest, (targets, repository, tag)) in seen {
        specs.push(EdgeSpec {
            from_kind: UNIT,
            from_id: key.to_string(),
            to_kind: ARTIFACT,
            to_id: digest.to_string(),
            relation: RUNS,
            confidence: Confidence::Exact,
            rule: UNIT_CONSUMED,
            evidence: json!({ "repository": repository, "tag": tag, "targets": targets }),
        });
    }
    for source in &subject.sources {
        let Some(commit) = &source.commit else {
            continue;
        };
        specs.push(EdgeSpec {
            from_kind: UNIT,
            from_id: key.to_string(),
            to_kind: COMMIT,
            to_id: commit_node_id(&source.repo, commit),
            relation: SYNCED,
            confidence: Confidence::Exact,
            rule: UNIT_SOURCE_COMMIT,
            evidence: json!({ "repo": source.repo, "commit": commit, "paths": source.scopes }),
        });
    }
    for target in &subject.targets {
        specs.push(EdgeSpec {
            from_kind: UNIT,
            from_id: key.to_string(),
            to_kind: TARGET,
            to_id: target.key(),
            relation: DEPLOYS_TO,
            confidence: Confidence::Exact,
            rule: UNIT_TARGET,
            evidence: json!({
                "cluster": target.cluster,
                "namespace": target.namespace,
                "name": target.name,
            }),
        });
    }
    specs
}

const RUN_MANAGED: [&str; 2] = [RUN_SOURCE_COMMIT, PROVIDER_PRODUCED];
const UNIT_MANAGED: [&str; 3] = [UNIT_CONSUMED, UNIT_SOURCE_COMMIT, UNIT_TARGET];
const UNIT_RELATIONS: [&str; 3] = [RUNS, SYNCED, DEPLOYS_TO];

impl CorrelationService {
    pub(super) async fn sync_exact_edges(
        &self, conn: &mut CorrelationConn, world: &World, affected: &Affected, now: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let mut changed = false;
        let pipelines: BTreeSet<String> = if affected.all {
            conn.list_run_evidence()
                .await?
                .into_iter()
                .map(|row| row.pipeline_id)
                .collect()
        } else {
            affected.subjects.clone()
        };
        for pipeline in &pipelines {
            changed |= self.sync_run_edges(conn, pipeline, now).await?;
        }
        for (key, subject) in world.unit_subjects() {
            if !affected.has(&subject.id) {
                continue;
            }
            let existing = conn
                .list_edges_from(UNIT, key)
                .await?
                .into_iter()
                .filter(|row| UNIT_RELATIONS.contains(&row.relation.as_str()))
                .collect();
            let desired = unit_specs(key, subject);
            let applied = apply(
                conn,
                existing,
                &desired,
                |row| UNIT_MANAGED.contains(&row.rule.as_str()),
                now,
            )
            .await?;
            changed |= applied.changed;
        }
        Ok(changed)
    }

    async fn sync_run_edges(
        &self, conn: &mut CorrelationConn, pipeline: &str, now: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let runs = conn.list_run_evidence_by_pipeline(pipeline).await?;
        if runs.is_empty() {
            return Ok(false);
        }
        let sources = conn.list_source_observations_by_subject(pipeline).await?;
        let artifacts = conn.list_artifact_observations_by_subject(pipeline).await?;
        let mut changed = false;
        for run in runs {
            let run_sources: Vec<&SourceObservationRow> = sources
                .iter()
                .filter(|r| r.run_number == run.run_number)
                .collect();
            let run_artifacts: Vec<&ArtifactObservationRow> = artifacts
                .iter()
                .filter(|r| {
                    r.run_number == run.run_number
                        && r.run_number != PIPELINE_LEVEL
                        && r.relation == super::vocab::REL_PRODUCED
                })
                .collect();
            let desired = run_specs(pipeline, run.run_number, &run_sources, &run_artifacts);
            let node = run_node_id(pipeline, run.run_number);
            let existing = conn.list_edges_from(RUN, &node).await?;
            let applied = apply(
                conn,
                existing,
                &desired,
                |row| RUN_MANAGED.contains(&row.rule.as_str()),
                now,
            )
            .await?;
            changed |= applied.changed;
        }
        Ok(changed)
    }
}
