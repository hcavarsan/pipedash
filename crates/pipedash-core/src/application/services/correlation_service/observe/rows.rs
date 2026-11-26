use std::collections::{
    BTreeSet,
    HashMap,
};

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    ArtifactObservation,
    Correlation,
    Observed,
    TargetLifetime,
    UnitId,
};

use crate::application::services::correlation_service::transitions::Snapshot;
use crate::application::services::correlation_service::vocab::{
    unit_node_id,
    LIFETIME_SHORT_LIVED,
    LIFETIME_STANDING,
    ORIGIN_PROVIDER,
    REL_CONSUMED,
    REL_PRODUCED,
};
use crate::domain::{
    DomainResult,
    PipelineRun,
};
use crate::infrastructure::database::{
    ArtifactObservationRow,
    ArtifactRow,
    CorrelationConn,
    RunEvidenceRow,
    SourceObservationRow,
    SubjectIdentityRow,
    TargetObservationRow,
};

#[derive(Default)]
pub(super) struct Rows {
    sources: Vec<SourceObservationRow>,
    targets: Vec<TargetObservationRow>,
    artifacts: Vec<ArtifactObservationRow>,
}

fn artifact_row(
    provider_id: i64, subject: &str, run: i64, relation: &str, artifact: &ArtifactObservation,
    target: Option<usize>,
) -> ArtifactObservationRow {
    ArtifactObservationRow {
        provider_id,
        subject_id: subject.to_string(),
        run_number: run,
        relation: relation.to_string(),
        ordinal: 0,
        repository: artifact.repository.as_str().to_string(),
        digest: artifact.digest.as_ref().map(|d| d.as_str().to_string()),
        tag: artifact.tag.clone(),
        target_ordinal: target.and_then(|t| i64::try_from(t).ok()),
        origin: ORIGIN_PROVIDER.to_string(),
    }
}

impl Rows {
    pub(super) fn of(provider_id: i64, subject: &str, run: i64, c: &Correlation) -> Self {
        let sources = c
            .sources
            .iter()
            .map(|s| SourceObservationRow {
                provider_id,
                subject_id: subject.to_string(),
                run_number: run,
                ordinal: 0,
                repo: s.repo.as_str().to_string(),
                path_scopes_json: serde_json::to_string(&s.path_scopes).unwrap_or_default(),
                commit_sha: s.commit.as_ref().map(|c| c.as_str().to_string()),
            })
            .collect();
        let targets = c
            .targets
            .iter()
            .map(|t| TargetObservationRow {
                provider_id,
                subject_id: subject.to_string(),
                run_number: run,
                ordinal: 0,
                cluster: t.cluster.as_str().to_string(),
                namespace: t.namespace.clone(),
                name: t.name.clone(),
                lifetime: match t.lifetime {
                    TargetLifetime::Standing => LIFETIME_STANDING,
                    TargetLifetime::ShortLived => LIFETIME_SHORT_LIVED,
                }
                .to_string(),
            })
            .collect();
        let consumed = c.consumed.iter().map(|a| {
            artifact_row(
                provider_id,
                subject,
                run,
                REL_CONSUMED,
                &a.artifact,
                a.target,
            )
        });
        let produced = c
            .produced
            .iter()
            .map(|a| artifact_row(provider_id, subject, run, REL_PRODUCED, a, None));
        Self {
            sources,
            targets,
            artifacts: consumed.chain(produced).collect(),
        }
    }

    pub(super) fn stored(
        sources: &[SourceObservationRow], targets: &[TargetObservationRow],
        artifacts: &[ArtifactObservationRow], run: i64,
    ) -> Self {
        Self {
            sources: sources
                .iter()
                .filter(|r| r.run_number == run)
                .cloned()
                .collect(),
            targets: targets
                .iter()
                .filter(|r| r.run_number == run)
                .cloned()
                .collect(),
            artifacts: artifacts
                .iter()
                .filter(|r| r.run_number == run && r.origin == ORIGIN_PROVIDER)
                .cloned()
                .collect(),
        }
    }

    pub(super) fn same_as(&self, other: &Rows) -> bool {
        let source = |r: &SourceObservationRow| {
            (
                r.repo.clone(),
                r.path_scopes_json.clone(),
                r.commit_sha.clone(),
            )
        };
        let target = |r: &TargetObservationRow| {
            (
                r.cluster.clone(),
                r.namespace.clone(),
                r.name.clone(),
                r.lifetime.clone(),
            )
        };
        let artifact = |r: &ArtifactObservationRow| {
            (
                r.relation.clone(),
                r.repository.clone(),
                r.digest.clone(),
                r.tag.clone(),
                r.target_ordinal,
            )
        };
        self.sources
            .iter()
            .map(source)
            .eq(other.sources.iter().map(source))
            && self
                .targets
                .iter()
                .map(target)
                .eq(other.targets.iter().map(target))
            && self
                .artifacts
                .iter()
                .map(artifact)
                .eq(other.artifacts.iter().map(artifact))
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        let digests: BTreeSet<String> = self
            .artifacts
            .iter()
            .filter(|r| r.relation == REL_CONSUMED)
            .filter_map(|r| r.digest.clone())
            .collect();
        Snapshot {
            revision: self.sources.first().and_then(|s| s.commit_sha.clone()),
            digests: digests.into_iter().collect(),
        }
    }

    pub(super) async fn write(
        &self, conn: &mut CorrelationConn, subject: &str, run: i64,
    ) -> DomainResult<()> {
        conn.replace_source_observations(subject, run, &self.sources)
            .await?;
        conn.replace_target_observations(subject, run, &self.targets)
            .await?;
        conn.replace_artifact_observations(subject, run, ORIGIN_PROVIDER, &self.artifacts)
            .await
    }

    pub(super) async fn ensure_artifacts(
        &self, conn: &mut CorrelationConn, now: DateTime<Utc>,
    ) -> DomainResult<()> {
        for row in &self.artifacts {
            let Some(digest) = &row.digest else {
                continue;
            };
            if conn.get_artifact(digest).await?.is_some() {
                continue;
            }
            conn.upsert_artifact(&ArtifactRow {
                digest: digest.clone(),
                repository: row.repository.clone(),
                first_seen: now,
                label_source: None,
                label_revision: None,
                registry_checked_at: None,
                log_scanned_at: None,
            })
            .await?;
        }
        Ok(())
    }

    pub(super) async fn neighbours(&self, conn: &mut CorrelationConn) -> DomainResult<Vec<String>> {
        let mut found = Vec::new();
        for source in &self.sources {
            let rows = conn.list_source_observations_by_repo(&source.repo).await?;
            found.extend(rows.into_iter().map(|r| r.subject_id));
        }
        for artifact in &self.artifacts {
            let Some(digest) = &artifact.digest else {
                continue;
            };
            let rows = conn.list_artifact_observations_by_digest(digest).await?;
            found.extend(rows.into_iter().map(|r| r.subject_id));
        }
        Ok(found)
    }
}

pub(super) fn identity_row(
    provider_id: i64, subject: &str, c: &Correlation,
) -> Option<SubjectIdentityRow> {
    if c.is_empty() {
        return None;
    }
    let unit = |u: &UnitId| unit_node_id(provider_id, u.as_str());
    let depends: Vec<String> = c.unit_depends_on.iter().map(unit).collect();
    Some(SubjectIdentityRow {
        subject_id: subject.to_string(),
        provider_id,
        unit_id: c.unit.as_ref().map(unit),
        unit_parent: c.unit_parent.as_ref().map(unit),
        unit_depends_on_json: serde_json::to_string(&depends).unwrap_or_default(),
        app_ref_authority: c.app_ref.as_ref().map(|a| a.authority.clone()),
        app_ref_scope: c.app_ref.as_ref().map(|a| a.scope.clone()),
        app_ref_id: c.app_ref.as_ref().map(|a| a.id.clone()),
        app_label: c.app_label.clone(),
    })
}

pub(super) fn evidence_row(pipeline: &str, run: &PipelineRun) -> RunEvidenceRow {
    RunEvidenceRow {
        pipeline_id: pipeline.to_string(),
        run_number: run.run_number,
        outcome_recorded: run.evidence.outcome == Observed::Recorded,
        timing_recorded: run.evidence.timing == Observed::Recorded,
        attempt: run.evidence.attempt,
        started_at: run.started_at,
        concluded_at: run.concluded_at,
        status: run.status.as_str().to_string(),
    }
}

pub(super) fn group_by_subject<R: Clone>(
    rows: &[R], subject: impl Fn(&R) -> &str,
) -> HashMap<String, Vec<R>> {
    let mut grouped: HashMap<String, Vec<R>> = HashMap::new();
    for row in rows {
        grouped
            .entry(subject(row).to_string())
            .or_default()
            .push(row.clone());
    }
    grouped
}
