use std::collections::{
    BTreeMap,
    BTreeSet,
};

use pipedash_plugin_api::{
    CommitSha,
    RepoId,
};

use super::context::Ctx;
use crate::application::services::correlation_service::changes::ChangeData;
use crate::application::services::correlation_service::page::live::{
    built_from,
    live_commits,
    primary,
    Live,
};
use crate::application::services::correlation_service::{
    copy,
    CorrelationService,
};
use crate::domain::application::metrics::{
    Coverage,
    MetricId,
    MetricResult,
    MetricUnit,
};
use crate::domain::DomainResult;

struct Heads<'a> {
    repo: &'a str,
    sha: &'a str,
    live_id: Option<i64>,
    green: &'a ChangeData,
}

fn live_commit<'a>(
    changes: &'a [ChangeData], live: &'a [Live], group_key: &str,
) -> Option<(&'a str, &'a str)> {
    if let Some(found) = primary(live) {
        return Some((&found.repo, &found.sha));
    }
    changes
        .iter()
        .flat_map(|change| change.deploys.iter().map(move |run| (run, change)))
        .filter(|(run, _)| run.group_key == group_key)
        .filter_map(|(run, change)| Some((run.succeeded_at()?, run.run_number, change)))
        .max_by_key(|(at, number, change)| (*at, *number, change.id))
        .map(|(_, _, change)| (change.repo.as_str(), change.commit_sha.as_str()))
}

fn heads<'a>(changes: &'a [ChangeData], live: &'a [Live], group_key: &str) -> Option<Heads<'a>> {
    let (repo, sha) = live_commit(changes, live, group_key)?;
    let green = changes
        .iter()
        .filter(|change| change.repo == repo)
        .filter_map(|change| Some((change.first_green_build()?.succeeded_at()?, change)))
        .max_by_key(|(at, change)| (*at, change.id))?
        .1;
    let live_id = changes
        .iter()
        .find(|change| change.repo == repo && change.commit_sha == sha)
        .map(|change| change.id);
    Some(Heads {
        repo,
        sha,
        live_id,
        green,
    })
}

pub(super) fn repo_id(stored: &str) -> Option<RepoId> {
    let (host, path) = stored.split_once('/')?;
    RepoId::from_host_path(host, path)
}

impl CorrelationService {
    async fn commits_behind(&self, heads: &Heads<'_>) -> Option<u32> {
        if heads.sha == heads.green.commit_sha {
            return Some(0);
        }
        let repo = repo_id(heads.repo)?;
        let live = CommitSha::parse(heads.sha)?;
        let green = CommitSha::parse(&heads.green.commit_sha)?;
        Some(self.compare(&repo, &live, &green).await?.ahead)
    }

    async fn live_of(&self, application_id: i64) -> DomainResult<BTreeMap<String, Vec<Live>>> {
        let mut conn = self.store.conn();
        let world = self.load_world(&mut conn).await?;
        let built = built_from(&mut conn).await?;
        let states = self
            .states_in(&mut conn, &world, Some(application_id))
            .await?;
        Ok(states
            .first()
            .map(|state| live_commits(&world, &built, &state.members, &state.targets))
            .unwrap_or_default())
    }

    pub(in crate::application::services::correlation_service) async fn behind_from(
        &self, application_id: i64, group_key: &str, live: &[Live],
    ) -> Option<u32> {
        let changes = self.all_changes(application_id).await.ok()?;
        self.commits_behind(&heads(&changes, live, group_key)?)
            .await
    }

    #[cfg(test)]
    pub(in crate::application::services::correlation_service) async fn behind_commits(
        &self, application_id: i64, group_key: &str,
    ) -> Option<u32> {
        let lives = self.live_of(application_id).await.ok()?;
        let live = lives.get(group_key).map_or(&[][..], Vec::as_slice);
        self.behind_from(application_id, group_key, live).await
    }

    pub(super) async fn drift_metric(&self, ctx: &Ctx, application_id: i64) -> MetricResult {
        let lives = self.live_of(application_id).await.unwrap_or_default();
        let mut behind: Vec<u32> = Vec::new();
        let mut measured: BTreeSet<i64> = BTreeSet::new();
        let mut skipped: BTreeSet<i64> = BTreeSet::new();
        let mut live_groups = 0;
        for group in ctx.groups.iter().filter(|g| ctx.group_visible(&g.key)) {
            let live = lives.get(&group.key).map_or(&[][..], Vec::as_slice);
            let Some(group_heads) = heads(&ctx.changes, live, &group.key) else {
                continue;
            };
            live_groups += 1;
            let ids = group_heads
                .live_id
                .into_iter()
                .chain([group_heads.green.id]);
            match self.commits_behind(&group_heads).await {
                Some(count) => {
                    behind.push(count);
                    measured.extend(ids);
                }
                None => skipped.extend(ids),
            }
        }
        let skipped_only: BTreeSet<i64> = skipped.difference(&measured).copied().collect();
        let value = behind.iter().max().map(|count| f64::from(*count));
        let reason = if live_groups == 0 {
            copy::no_deployments()
        } else {
            copy::no_commit_times()
        };
        MetricResult {
            id: MetricId::Drift,
            label: copy::metric_label(MetricId::Drift),
            unit: MetricUnit::Commits,
            value,
            p50: None,
            p95: None,
            unavailable_reason: value.is_none().then_some(reason),
            coverage: Coverage {
                counted: measured.len(),
                excluded: skipped_only.len(),
                since: ctx.since(),
                label: copy::coverage(measured.len(), skipped_only.len()),
            },
            series: Vec::new(),
        }
    }
}
