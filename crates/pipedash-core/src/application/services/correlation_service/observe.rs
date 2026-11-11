mod rows;

use std::collections::{
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Utc,
};
use rows::{
    evidence_row,
    group_by_subject,
    identity_row,
    Rows,
};

use super::resolve::finish;
use super::vocab::{
    run_node_id,
    PIPELINE,
    RUN,
    UNIT,
};
use super::CorrelationService;
use crate::domain::pipeline_id::SEPARATOR;
use crate::domain::{
    DomainResult,
    Pipeline,
    PipelineRun,
};
use crate::infrastructure::database::{
    CorrelationConn,
    RunEvidenceRow,
    SubjectIdentityRow,
    PIPELINE_LEVEL,
};

impl CorrelationService {
    async fn ignored(&self, provider_name: &str, pipeline: &str) -> bool {
        let config = self.config_state.get().await;
        let local = pipeline.splitn(3, SEPARATOR).nth(2).unwrap_or(pipeline);
        config
            .correlation
            .ignore
            .iter()
            .any(|selector| selector.matches(provider_name, local))
    }

    pub(super) async fn pipelines_applied_impl(
        &self, provider_id: i64, old: &[Pipeline], new: &[Pipeline],
    ) -> DomainResult<()> {
        self.pipelines_applied_at(provider_id, old, new, Utc::now())
            .await
    }

    pub(super) async fn pipelines_applied_at(
        &self, provider_id: i64, old: &[Pipeline], new: &[Pipeline], now: DateTime<Utc>,
    ) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let provider = self.repository.get_provider(provider_id).await?;
        let mut kept = Vec::new();
        for pipeline in new {
            if !self.ignored(&provider.name, &pipeline.id).await {
                kept.push(pipeline);
            }
        }
        let mut tx = self.store.begin().await?;
        let result = self
            .observe_pipelines(
                &mut tx,
                provider_id,
                provider.refresh_interval,
                old,
                &kept,
                now,
            )
            .await;
        let changed = finish(tx, result).await?;
        if !changed.is_empty() {
            self.resolve(&changed).await?;
            self.schedule_enrichment(&changed);
        }
        Ok(())
    }

    async fn observe_pipelines(
        &self, conn: &mut CorrelationConn, provider_id: i64, refresh_interval: i64,
        old: &[Pipeline], new: &[&Pipeline], now: DateTime<Utc>,
    ) -> DomainResult<HashSet<String>> {
        self.touch_window(conn, provider_id, refresh_interval, now)
            .await?;
        let mut identities: HashMap<String, SubjectIdentityRow> = conn
            .list_subject_identities_by_provider(provider_id)
            .await?
            .into_iter()
            .map(|row| (row.subject_id.clone(), row))
            .collect();
        let sources = conn.list_pipeline_sources(Some(provider_id)).await?;
        let targets = conn.list_pipeline_targets(Some(provider_id)).await?;
        let artifacts = conn.list_pipeline_artifacts(Some(provider_id)).await?;
        let by_sources = group_by_subject(&sources, |r| &r.subject_id);
        let by_targets = group_by_subject(&targets, |r| &r.subject_id);
        let by_artifacts = group_by_subject(&artifacts, |r| &r.subject_id);
        let old_by_id: HashMap<&str, &Pipeline> = old.iter().map(|p| (p.id.as_str(), p)).collect();
        let mut changed = HashSet::new();
        for pipeline in new {
            let id = pipeline.id.as_str();
            let stored = Rows::stored(
                by_sources.get(id).map_or(&[], Vec::as_slice),
                by_targets.get(id).map_or(&[], Vec::as_slice),
                by_artifacts.get(id).map_or(&[], Vec::as_slice),
                PIPELINE_LEVEL,
            );
            let rows = Rows::of(provider_id, id, PIPELINE_LEVEL, &pipeline.correlation);
            let identity = identity_row(provider_id, id, &pipeline.correlation);
            let previous = old_by_id.get(id).copied();
            if pipeline.correlation.unit.is_some() {
                let before = stored.snapshot();
                self.record_transition(conn, provider_id, now, previous, pipeline, &before)
                    .await?;
            }
            let rows_changed = !rows.same_as(&stored);
            if rows_changed {
                rows.write(conn, id, PIPELINE_LEVEL).await?;
                rows.ensure_artifacts(conn, now).await?;
                changed.extend(stored.neighbours(conn).await?);
            }
            let identity_changed = identity != identities.remove(id);
            if identity_changed {
                match &identity {
                    Some(row) => conn.upsert_subject_identity(row).await?,
                    None => {
                        conn.delete_subject_identity(id).await?;
                    }
                }
            }
            let renamed = previous.is_some_and(|p| p.name != pipeline.name);
            if rows_changed || identity_changed || renamed {
                changed.insert(id.to_string());
            }
        }
        for (id, identity) in identities {
            let stored = Rows::stored(
                by_sources.get(&id).map_or(&[], Vec::as_slice),
                &[],
                by_artifacts.get(&id).map_or(&[], Vec::as_slice),
                PIPELINE_LEVEL,
            );
            changed.extend(stored.neighbours(conn).await?);
            self.forget(conn, &id, identity.unit_id.as_deref()).await?;
            changed.insert(id);
        }
        Ok(changed)
    }

    async fn forget(
        &self, conn: &mut CorrelationConn, id: &str, unit: Option<&str>,
    ) -> DomainResult<()> {
        for run in conn.list_run_evidence_by_pipeline(id).await? {
            let node = run_node_id(id, run.run_number);
            conn.delete_edges_from(RUN, &node).await?;
        }
        conn.delete_edges_from(PIPELINE, id).await?;
        if let Some(unit) = unit {
            conn.delete_edges_from(UNIT, unit).await?;
            conn.delete_edges_to(UNIT, unit).await?;
        }
        conn.delete_source_observations_by_subject(id).await?;
        conn.delete_target_observations_by_subject(id).await?;
        conn.delete_artifact_observations_by_subject(id).await?;
        conn.delete_subject_identity(id).await?;
        Ok(())
    }

    pub(super) async fn runs_fetched_impl(
        &self, provider_id: i64, pipeline_id: &str, runs: &[PipelineRun],
    ) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let provider = self.repository.get_provider(provider_id).await?;
        if self.ignored(&provider.name, pipeline_id).await {
            return Ok(());
        }
        let mut tx = self.store.begin().await?;
        let result = self
            .observe_runs(&mut tx, provider_id, pipeline_id, runs, Utc::now())
            .await;
        if finish(tx, result).await? {
            let changed = HashSet::from([pipeline_id.to_string()]);
            self.resolve(&changed).await?;
            self.schedule_enrichment(&changed);
        } else {
            self.schedule_changes_for(HashSet::from([pipeline_id.to_string()]));
        }
        Ok(())
    }

    async fn observe_runs(
        &self, conn: &mut CorrelationConn, provider_id: i64, pipeline: &str, runs: &[PipelineRun],
        now: DateTime<Utc>,
    ) -> DomainResult<bool> {
        let tracked = runs.iter().any(|r| !r.correlation.is_empty())
            || !conn
                .list_application_members_by_pipeline(pipeline)
                .await?
                .is_empty()
            || conn.get_subject_identity(pipeline).await?.is_some();
        if !tracked {
            return Ok(false);
        }
        let evidence: HashMap<i64, RunEvidenceRow> = conn
            .list_run_evidence_by_pipeline(pipeline)
            .await?
            .into_iter()
            .map(|row| (row.run_number, row))
            .collect();
        let sources = conn.list_source_observations_by_subject(pipeline).await?;
        let targets = conn.list_target_observations_by_subject(pipeline).await?;
        let artifacts = conn.list_artifact_observations_by_subject(pipeline).await?;
        let mut graph_changed = false;
        for run in runs {
            let row = evidence_row(pipeline, run);
            if evidence.get(&run.run_number) != Some(&row) {
                conn.upsert_run_evidence(&row).await?;
            }
            let rows = Rows::of(provider_id, pipeline, run.run_number, &run.correlation);
            let stored = Rows::stored(&sources, &targets, &artifacts, run.run_number);
            if !rows.same_as(&stored) {
                rows.write(conn, pipeline, run.run_number).await?;
                rows.ensure_artifacts(conn, now).await?;
                graph_changed = true;
            }
        }
        Ok(graph_changed)
    }
}
