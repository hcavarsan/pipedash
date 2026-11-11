use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Utc,
};

use crate::application::services::correlation_service::changes::{
    ChangeData,
    StageRun,
};
use crate::application::services::pipeline_service::overdue::provider_id_of;
use crate::domain::application::dto::ChangeColumn;
use crate::domain::metrics::AggregationPeriod;
use crate::domain::pipeline::PipelineStatus;
use crate::infrastructure::database::ObservedTransitionRow;

pub(super) type Window = (DateTime<Utc>, DateTime<Utc>);
pub(super) type AncestryKey = (String, String, String);

pub(super) struct Journey<'a> {
    pub(super) change: &'a ChangeData,
    pub(super) group: &'a str,
    pub(super) deploy: &'a StageRun,
    pub(super) deployed_at: DateTime<Utc>,
}

pub(super) struct Ctx {
    pub(super) from: DateTime<Utc>,
    pub(super) to: DateTime<Utc>,
    pub(super) period: AggregationPeriod,
    pub(super) group_filter: Option<String>,
    pub(super) changes: Vec<ChangeData>,
    pub(super) short_lived: HashSet<String>,
    pub(super) groups: Vec<ChangeColumn>,
    pub(super) directions: Vec<(String, String)>,
    pub(super) transitions: HashMap<String, Vec<ObservedTransitionRow>>,
    pub(super) windows: HashMap<i64, Vec<Window>>,
    pub(super) digests: HashMap<(String, i64), BTreeSet<String>>,
    pub(super) run_commits: HashMap<(String, i64), BTreeSet<String>>,
    pub(super) ancestors: HashMap<AncestryKey, bool>,
}

impl Ctx {
    pub(super) fn in_range(&self, at: DateTime<Utc>) -> bool {
        self.from <= at && at <= self.to
    }

    pub(super) fn group_visible(&self, key: &str) -> bool {
        !self.short_lived.contains(key) && self.group_filter.as_deref().is_none_or(|g| g == key)
    }

    pub(super) fn deploys<'a>(
        &'a self, change: &'a ChangeData,
    ) -> impl Iterator<Item = &'a StageRun> + 'a {
        change
            .deploys
            .iter()
            .filter(|run| self.group_visible(&run.group_key))
    }

    pub(super) fn in_scope(&self, change: &ChangeData) -> bool {
        change.builds.iter().chain(&change.deploys).any(|run| {
            self.in_range(run.started_at) || run.concluded_at.is_some_and(|at| self.in_range(at))
        })
    }

    pub(super) fn scope_total(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| self.in_scope(change))
            .count()
    }

    pub(super) fn since(&self) -> Option<DateTime<Utc>> {
        self.changes
            .iter()
            .flat_map(|change| change.builds.iter().chain(&change.deploys))
            .map(|run| run.started_at)
            .min()
    }

    pub(super) fn window_since(&self) -> Option<DateTime<Utc>> {
        self.windows
            .values()
            .flatten()
            .map(|(started, _)| *started)
            .min()
    }

    pub(super) fn window_at(&self, pipeline_id: &str, at: DateTime<Utc>) -> Option<Window> {
        let provider = provider_id_of(pipeline_id)?;
        self.windows
            .get(&provider)?
            .iter()
            .find(|(started, ended)| *started <= at && at <= *ended)
            .copied()
    }

    pub(super) fn status_at(&self, pipeline_id: &str, at: DateTime<Utc>) -> Option<&str> {
        let rows = self.transitions.get(pipeline_id)?;
        let seen = rows.partition_point(|row| row.at <= at);
        seen.checked_sub(1)
            .map(|index| rows[index].status_after.as_str())
    }

    fn healthy(&self, run: &StageRun, at: DateTime<Utc>) -> bool {
        if self.window_at(&run.pipeline_id, at).is_none() {
            return true;
        }
        self.status_at(&run.pipeline_id, at).is_none_or(|status| {
            status != PipelineStatus::Failed.as_str() && status != PipelineStatus::Degraded.as_str()
        })
    }

    pub(super) fn journeys(&self) -> Vec<Journey<'_>> {
        let mut out = Vec::new();
        for change in &self.changes {
            let groups: BTreeSet<&str> = self
                .deploys(change)
                .map(|run| run.group_key.as_str())
                .collect();
            for group in groups {
                let mut rows: Vec<(DateTime<Utc>, &StageRun)> = self
                    .deploys(change)
                    .filter(|run| run.group_key == group)
                    .filter_map(|run| Some((run.succeeded_at()?, run)))
                    .collect();
                rows.sort_by_key(|(at, run)| (*at, run.run_number));
                if let Some((deployed_at, deploy)) =
                    rows.into_iter().find(|(at, run)| self.healthy(run, *at))
                {
                    out.push(Journey {
                        change,
                        group,
                        deploy,
                        deployed_at,
                    });
                }
            }
        }
        out
    }

    pub(super) fn upstream_groups(&self, group: &str) -> BTreeSet<&str> {
        self.directions
            .iter()
            .filter(|(_, to)| to == group)
            .map(|(from, _)| from.as_str())
            .collect()
    }

    pub(super) fn identity(
        &self, change: &ChangeData, run: &StageRun,
    ) -> (BTreeSet<String>, BTreeSet<String>) {
        let key = (run.pipeline_id.clone(), run.run_number);
        let mut commits = self.run_commits.get(&key).cloned().unwrap_or_default();
        commits.insert(change.commit_sha.clone());
        (commits, self.digests.get(&key).cloned().unwrap_or_default())
    }
}

pub(super) fn by_group<'a>(
    rows: impl Iterator<Item = &'a StageRun>,
) -> BTreeMap<&'a str, Vec<&'a StageRun>> {
    let mut groups: BTreeMap<&str, Vec<&StageRun>> = BTreeMap::new();
    for run in rows {
        groups.entry(run.group_key.as_str()).or_default().push(run);
    }
    groups
}
