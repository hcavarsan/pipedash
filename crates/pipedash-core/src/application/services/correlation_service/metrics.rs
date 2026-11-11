mod context;
mod drift;
mod durations;
mod failures;
mod rates;
mod series;
#[cfg(test)]
mod tests;

use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
};

use pipedash_plugin_api::CommitSha;

use self::context::Ctx;
use super::vocab::{
    LIFETIME_SHORT_LIVED,
    REL_CONSUMED,
};
use super::CorrelationService;
use crate::application::services::pipeline_service::overdue::provider_id_of;
use crate::domain::application::metrics::{
    ApplicationMetrics,
    ApplicationMetricsQuery,
};
use crate::domain::{
    DomainError,
    DomainResult,
};

impl CorrelationService {
    pub async fn metrics(
        &self, query: ApplicationMetricsQuery,
    ) -> DomainResult<ApplicationMetrics> {
        let mut ctx = self.metrics_context(&query).await?;
        let timelines = failures::timelines(&ctx);
        for key in failures::ancestry_keys(&ctx, &timelines) {
            let ancestor = self.is_ancestor(&key).await;
            ctx.ancestors.insert(key, ancestor);
        }
        let journeys: Vec<_> = ctx
            .journeys()
            .into_iter()
            .filter(|journey| ctx.in_range(journey.deployed_at))
            .collect();
        let metrics = vec![
            durations::lead_time(&ctx, &journeys),
            durations::delivery_time(&ctx, &journeys),
            durations::build_time(&ctx),
            durations::wait_time(&ctx, &journeys),
            durations::build_to_deployed(&ctx, &journeys),
            durations::deploy_time(&ctx, &journeys),
            rates::deploy_frequency(&ctx),
            rates::build_success_rate(&ctx),
            rates::deploy_failures(&ctx),
            failures::change_failure_rate(&ctx),
            failures::recovery_time(&ctx),
            self.drift_metric(&ctx).await,
        ];
        Ok(ApplicationMetrics {
            metrics,
            funnel: rates::funnel(&ctx),
            groups: ctx
                .groups
                .iter()
                .filter(|group| !ctx.short_lived.contains(&group.key))
                .cloned()
                .collect(),
        })
    }

    async fn is_ancestor(&self, key: &context::AncestryKey) -> bool {
        let (repo, base, head) = key;
        let (Some(repo), Some(base), Some(head)) = (
            drift::repo_id(repo),
            CommitSha::parse(base),
            CommitSha::parse(head),
        ) else {
            return false;
        };
        self.compare(&repo, &base, &head)
            .await
            .is_some_and(|comparison| comparison.behind > 0 && comparison.ahead == 0)
    }

    async fn metrics_context(&self, query: &ApplicationMetricsQuery) -> DomainResult<Ctx> {
        let id = query.application_id;
        let mut conn = self.store.conn();
        if conn.get_application(id).await?.is_none() {
            return Err(DomainError::NotFound(format!("Application {id}")));
        }
        let changes = self.all_changes(id).await?;
        let groups = self.group_columns(id).await?;
        let mut standing: BTreeMap<String, bool> = BTreeMap::new();
        for target in conn.list_application_targets_by_application(id).await? {
            let live = standing.entry(target.group_key).or_insert(false);
            *live |= target.lifetime != LIFETIME_SHORT_LIVED;
        }
        let directions = conn
            .list_target_directions_by_application(id)
            .await?
            .into_iter()
            .map(|row| (row.from_group, row.to_group))
            .collect();
        let pipelines: BTreeSet<&str> = changes
            .iter()
            .flat_map(|change| &change.deploys)
            .map(|run| run.pipeline_id.as_str())
            .collect();
        let mut ctx = Ctx {
            from: query.from,
            to: query.to,
            period: query.period,
            group_filter: query.group_key.clone(),
            short_lived: standing
                .into_iter()
                .filter(|(_, live)| !live)
                .map(|(group, _)| group)
                .collect(),
            groups,
            directions,
            transitions: HashMap::new(),
            windows: HashMap::new(),
            digests: HashMap::new(),
            run_commits: HashMap::new(),
            ancestors: HashMap::new(),
            changes: Vec::new(),
        };
        for pipeline in pipelines {
            ctx.transitions.insert(
                pipeline.to_string(),
                conn.list_observed_transitions_by_pipeline(pipeline).await?,
            );
            for row in conn.list_artifact_observations_by_subject(pipeline).await? {
                if let (REL_CONSUMED, Some(digest)) = (row.relation.as_str(), row.digest) {
                    ctx.digests
                        .entry((pipeline.to_string(), row.run_number))
                        .or_default()
                        .insert(digest);
                }
            }
            for row in conn.list_source_observations_by_subject(pipeline).await? {
                if let Some(commit) = row.commit_sha {
                    ctx.run_commits
                        .entry((pipeline.to_string(), row.run_number))
                        .or_default()
                        .insert(commit);
                }
            }
        }
        let providers: BTreeSet<i64> = ctx
            .transitions
            .keys()
            .filter_map(|pipeline| provider_id_of(pipeline))
            .collect();
        for provider in providers {
            let windows = conn
                .list_observation_windows_by_provider(provider)
                .await?
                .into_iter()
                .map(|row| (row.started_at, row.ended_at))
                .collect();
            ctx.windows.insert(provider, windows);
        }
        ctx.changes = changes;
        Ok(ctx)
    }
}
