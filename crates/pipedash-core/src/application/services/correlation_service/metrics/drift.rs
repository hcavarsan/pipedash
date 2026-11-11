use std::collections::BTreeSet;

use pipedash_plugin_api::{
    CommitSha,
    RepoId,
};

use super::context::Ctx;
use crate::application::services::correlation_service::changes::ChangeData;
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

struct Heads<'a> {
    live: &'a ChangeData,
    green: &'a ChangeData,
}

fn heads<'a>(changes: &'a [ChangeData], group_key: &str) -> Option<Heads<'a>> {
    let live = changes
        .iter()
        .filter_map(|change| Some((change.first_deploy(group_key)?.succeeded_at()?, change)))
        .max_by_key(|(at, change)| (*at, change.id))?
        .1;
    let green = changes
        .iter()
        .filter(|change| change.repo == live.repo)
        .filter_map(|change| Some((change.first_green_build()?.succeeded_at()?, change)))
        .max_by_key(|(at, change)| (*at, change.id))?
        .1;
    Some(Heads { live, green })
}

pub(super) fn repo_id(stored: &str) -> Option<RepoId> {
    let (host, path) = stored.split_once('/')?;
    RepoId::from_host_path(host, path)
}

impl CorrelationService {
    async fn commits_behind(&self, heads: &Heads<'_>) -> Option<u32> {
        if heads.live.commit_sha == heads.green.commit_sha {
            return Some(0);
        }
        let repo = repo_id(&heads.live.repo)?;
        let live = CommitSha::parse(&heads.live.commit_sha)?;
        let green = CommitSha::parse(&heads.green.commit_sha)?;
        Some(self.compare(&repo, &live, &green).await?.ahead)
    }

    pub(in crate::application::services::correlation_service) async fn behind_commits(
        &self, application_id: i64, group_key: &str,
    ) -> Option<u32> {
        let changes = self.all_changes(application_id).await.ok()?;
        self.commits_behind(&heads(&changes, group_key)?).await
    }

    pub(super) async fn drift_metric(&self, ctx: &Ctx) -> MetricResult {
        let mut behind: Vec<u32> = Vec::new();
        let mut measured: BTreeSet<i64> = BTreeSet::new();
        let mut skipped: BTreeSet<i64> = BTreeSet::new();
        let mut live_groups = 0;
        for group in ctx.groups.iter().filter(|g| ctx.group_visible(&g.key)) {
            let Some(group_heads) = heads(&ctx.changes, &group.key) else {
                continue;
            };
            live_groups += 1;
            let ids = [group_heads.live.id, group_heads.green.id];
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
