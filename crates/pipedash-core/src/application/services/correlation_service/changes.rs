mod derive;
pub(super) mod segments;
#[cfg(test)]
mod tests;

use std::collections::{
    BTreeMap,
    BTreeSet,
    HashSet,
};

use chrono::{
    DateTime,
    Utc,
};

use super::CorrelationService;
use crate::domain::application::dto::ChangeColumn;
use crate::domain::application::Stage;
use crate::domain::pipeline::PipelineStatus;
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ChangeRow,
    ChangeStageRow,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StageRun {
    pub(super) group_key: String,
    pub(super) pipeline_id: String,
    pub(super) run_number: i64,
    pub(super) started_at: DateTime<Utc>,
    pub(super) concluded_at: Option<DateTime<Utc>>,
    pub(super) status: String,
    pub(super) outcome_recorded: bool,
    pub(super) timing_recorded: bool,
    pub(super) attempt: bool,
}

impl StageRun {
    fn new(row: ChangeStageRow) -> Self {
        Self {
            group_key: row.group_key,
            pipeline_id: row.pipeline_id,
            run_number: row.run_number,
            started_at: row.started_at,
            concluded_at: row.concluded_at,
            status: row.status,
            outcome_recorded: row.outcome_recorded,
            timing_recorded: row.timing_recorded,
            attempt: row.attempt,
        }
    }

    pub(super) fn is_success(&self) -> bool {
        self.status == PipelineStatus::Success.as_str()
    }

    pub(super) fn is_failed(&self) -> bool {
        self.status == PipelineStatus::Failed.as_str()
    }

    pub(super) fn is_concluded(&self) -> bool {
        self.concluded_at.is_some() && (self.is_success() || self.is_failed())
    }

    pub(super) fn succeeded_at(&self) -> Option<DateTime<Utc>> {
        self.concluded_at
            .filter(|_| !self.outcome_recorded || self.is_success())
    }

    pub(super) fn timed_span(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let end = self.concluded_at?;
        (self.timing_recorded && end >= self.started_at).then_some((self.started_at, end))
    }

    pub(super) fn event_at(&self) -> DateTime<Utc> {
        self.concluded_at.unwrap_or(self.started_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChangeData {
    pub(super) id: i64,
    pub(super) repo: String,
    pub(super) commit_sha: String,
    pub(super) authored_at: Option<DateTime<Utc>>,
    pub(super) builds: Vec<StageRun>,
    pub(super) deploys: Vec<StageRun>,
}

impl ChangeData {
    pub(super) fn first_deploy(&self, group_key: &str) -> Option<&StageRun> {
        self.deploys
            .iter()
            .filter(|run| run.group_key == group_key && run.succeeded_at().is_some())
            .min_by_key(|run| (run.succeeded_at(), run.run_number))
    }

    pub(super) fn first_green_build(&self) -> Option<&StageRun> {
        self.builds
            .iter()
            .filter(|run| run.succeeded_at().is_some())
            .min_by_key(|run| (run.succeeded_at(), run.run_number))
    }
}

fn assemble(rows: Vec<ChangeRow>, stages: Vec<ChangeStageRow>) -> Vec<ChangeData> {
    let mut by_change: BTreeMap<i64, Vec<ChangeStageRow>> = BTreeMap::new();
    for stage in stages {
        by_change.entry(stage.change_id).or_default().push(stage);
    }
    rows.into_iter()
        .map(|row| {
            let mut data = ChangeData {
                id: row.id,
                repo: row.repo,
                commit_sha: row.commit_sha,
                authored_at: row.authored_at,
                builds: Vec::new(),
                deploys: Vec::new(),
            };
            for stage in by_change.remove(&row.id).unwrap_or_default() {
                match Stage::parse(&stage.stage) {
                    Some(Stage::Build) => data.builds.push(StageRun::new(stage)),
                    Some(Stage::Deploy) => data.deploys.push(StageRun::new(stage)),
                    None => {}
                }
            }
            data
        })
        .collect()
}

impl CorrelationService {
    pub(super) async fn rebuild_changes(&self, application_id: i64) -> DomainResult<()> {
        if self.rebuild_changes_impl(application_id).await? {
            self.changed();
        }
        Ok(())
    }

    pub(super) fn schedule_changes(&self, applications: Vec<i64>) {
        let Some(service) = self.arc().filter(|_| !applications.is_empty()) else {
            return;
        };
        self.spawn(async move {
            let _guard = service.write_lock.lock().await;
            for application in applications {
                if let Err(error) = service.rebuild_changes(application).await {
                    tracing::warn!(%error, application, "Could not rebuild changes");
                }
            }
        });
    }

    pub(super) fn schedule_changes_for(&self, pipelines: HashSet<String>) {
        let Some(service) = self.arc() else {
            return;
        };
        self.spawn(async move {
            let _guard = service.write_lock.lock().await;
            let mut applications = BTreeSet::new();
            for pipeline in &pipelines {
                let members = service
                    .store
                    .conn()
                    .list_application_members_by_pipeline(pipeline)
                    .await;
                match members {
                    Ok(rows) => applications.extend(rows.into_iter().map(|m| m.application_id)),
                    Err(error) => tracing::warn!(%error, pipeline, "Could not read members"),
                }
            }
            for application in applications {
                if let Err(error) = service.rebuild_changes(application).await {
                    tracing::warn!(%error, application, "Could not rebuild changes");
                }
            }
        });
    }

    pub(super) async fn all_changes(&self, application_id: i64) -> DomainResult<Vec<ChangeData>> {
        let mut conn = self.store.conn();
        let rows = conn.list_changes_by_application(application_id).await?;
        let stages = conn
            .list_change_stages_by_application(application_id)
            .await?;
        Ok(assemble(rows, stages))
    }

    pub(super) async fn changes_page(
        &self, application_id: i64, limit: i64, offset: i64,
    ) -> DomainResult<Vec<ChangeData>> {
        let mut conn = self.store.conn();
        let rows = conn
            .list_changes_page(application_id, limit, offset)
            .await?;
        let mut stages = Vec::new();
        for row in &rows {
            stages.extend(conn.list_change_stages_by_change(row.id).await?);
        }
        Ok(assemble(rows, stages))
    }

    pub(super) async fn group_columns(
        &self, application_id: i64,
    ) -> DomainResult<Vec<ChangeColumn>> {
        let targets = self
            .store
            .conn()
            .list_application_targets_by_application(application_id)
            .await?;
        let mut groups: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
        for target in targets {
            groups
                .entry(target.group_key)
                .or_default()
                .push((target.name, target.lifetime == "short_lived"));
        }
        Ok(groups
            .into_iter()
            .filter(|(_, targets)| targets.iter().any(|(_, short_lived)| !short_lived))
            .map(|(key, targets)| ChangeColumn {
                label: group_label(&key, &targets),
                key,
            })
            .collect())
    }
}

pub(super) fn group_label(key: &str, targets: &[(String, bool)]) -> String {
    match targets {
        [(name, _)] => name.clone(),
        _ => key
            .split_once(':')
            .map_or(key, |(_, label)| label)
            .to_string(),
    }
}
