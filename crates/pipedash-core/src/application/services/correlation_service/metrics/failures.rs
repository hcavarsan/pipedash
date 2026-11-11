use std::collections::{
    BTreeMap,
    BTreeSet,
    HashSet,
};

use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};

use super::context::{
    by_group,
    AncestryKey,
    Ctx,
};
use super::series::{
    summarize,
    Sample,
    Spec,
};
use crate::application::services::correlation_service::changes::{
    ChangeData,
    StageRun,
};
use crate::application::services::correlation_service::{
    copy,
    FAILURE_WINDOW_MINUTES,
};
use crate::domain::application::metrics::{
    MetricId,
    MetricResult,
};
use crate::domain::pipeline::PipelineStatus;
use crate::infrastructure::database::ObservedTransitionRow;

type Row<'a> = (DateTime<Utc>, i64, &'a ChangeData, &'a StageRun);

pub(super) struct Segment {
    change_id: i64,
    repo: String,
    commit: String,
    first_at: DateTime<Utc>,
    pipeline_id: String,
    digests: BTreeSet<String>,
}

struct Deployed<'a> {
    change_id: i64,
    at: DateTime<Utc>,
    failed: bool,
    failure: Option<&'a ObservedTransitionRow>,
}

fn unhealthy(status: &str) -> bool {
    status == PipelineStatus::Failed.as_str() || status == PipelineStatus::Degraded.as_str()
}

pub(super) fn timelines(ctx: &Ctx) -> BTreeMap<String, Vec<Segment>> {
    let mut rows: BTreeMap<&str, Vec<Row<'_>>> = BTreeMap::new();
    for change in &ctx.changes {
        for run in ctx.deploys(change) {
            if let Some(at) = run.succeeded_at() {
                rows.entry(run.group_key.as_str()).or_default().push((
                    at,
                    run.run_number,
                    change,
                    run,
                ));
            }
        }
    }
    rows.into_iter()
        .map(|(group, mut rows)| {
            rows.sort_by_key(|(at, run_number, change, _)| (*at, *run_number, change.id));
            let mut segments: Vec<Segment> = Vec::new();
            for (at, _, change, run) in rows {
                let (_, digests) = ctx.identity(change, run);
                match segments.last_mut() {
                    Some(last) if last.change_id == change.id => last.digests.extend(digests),
                    _ => segments.push(Segment {
                        change_id: change.id,
                        repo: change.repo.clone(),
                        commit: change.commit_sha.clone(),
                        first_at: at,
                        pipeline_id: run.pipeline_id.clone(),
                        digests,
                    }),
                }
            }
            (group.to_string(), segments)
        })
        .collect()
}

fn digest_rollback(segments: &[Segment], index: usize) -> bool {
    let (current, next) = (&segments[index], &segments[index + 1]);
    if next.digests.is_empty() || !next.digests.is_disjoint(&current.digests) {
        return false;
    }
    segments[..index]
        .iter()
        .any(|earlier| !earlier.digests.is_disjoint(&next.digests))
}

fn ancestry_key(segments: &[Segment], index: usize) -> Option<AncestryKey> {
    let (current, next) = (&segments[index], &segments[index + 1]);
    (current.repo == next.repo && current.commit != next.commit).then(|| {
        (
            current.repo.clone(),
            current.commit.clone(),
            next.commit.clone(),
        )
    })
}

pub(super) fn ancestry_keys(
    ctx: &Ctx, timelines: &BTreeMap<String, Vec<Segment>>,
) -> BTreeSet<AncestryKey> {
    let mut keys = BTreeSet::new();
    for segments in timelines.values() {
        for index in 0..segments.len().saturating_sub(1) {
            let current = &segments[index];
            let inside = ctx.in_range(current.first_at)
                && ctx
                    .window_at(&current.pipeline_id, current.first_at)
                    .is_some();
            if !inside || digest_rollback(segments, index) {
                continue;
            }
            keys.extend(ancestry_key(segments, index));
        }
    }
    keys
}

fn rolled_back(ctx: &Ctx, timelines: &BTreeMap<String, Vec<Segment>>) -> HashSet<(i64, String)> {
    let mut rolled = HashSet::new();
    for (group, segments) in timelines {
        for index in 0..segments.len().saturating_sub(1) {
            let by_ancestry = ancestry_key(segments, index)
                .is_some_and(|key| ctx.ancestors.get(&key).copied().unwrap_or(false));
            if digest_rollback(segments, index) || by_ancestry {
                rolled.insert((segments[index].change_id, group.clone()));
            }
        }
    }
    rolled
}

fn lists(
    row: &ObservedTransitionRow, commits: &BTreeSet<String>, digests: &BTreeSet<String>,
) -> bool {
    let by_revision = row
        .revision
        .as_deref()
        .is_some_and(|revision| commits.contains(revision));
    let running: Vec<String> = serde_json::from_str(&row.digests_json).unwrap_or_default();
    by_revision || running.iter().any(|digest| digests.contains(digest))
}

fn failure_transition<'a>(
    ctx: &'a Ctx, change: &ChangeData, rows: &[&StageRun],
) -> Option<&'a ObservedTransitionRow> {
    let (deployed, run) = rows
        .iter()
        .filter_map(|run| Some((run.succeeded_at()?, *run)))
        .min_by_key(|(at, run)| (*at, run.run_number))?;
    let (commits, digests) = ctx.identity(change, run);
    let until = deployed + TimeDelta::minutes(FAILURE_WINDOW_MINUTES);
    ctx.transitions.get(&run.pipeline_id)?.iter().find(|row| {
        row.at >= deployed
            && row.at <= until
            && unhealthy(&row.status_after)
            && lists(row, &commits, &digests)
    })
}

fn deployed_changes(ctx: &Ctx) -> Vec<Deployed<'_>> {
    let rolled = rolled_back(ctx, &timelines(ctx));
    let mut out = Vec::new();
    for change in &ctx.changes {
        for (group, rows) in by_group(ctx.deploys(change)) {
            let Some(first) = rows
                .iter()
                .min_by_key(|run| (run.event_at(), run.run_number))
            else {
                continue;
            };
            let at = first.event_at();
            if ctx.window_at(&first.pipeline_id, at).is_none() {
                continue;
            }
            let attempt_failed = rows
                .iter()
                .any(|run| run.attempt && run.outcome_recorded && run.is_failed());
            let failure = failure_transition(ctx, change, &rows);
            out.push(Deployed {
                change_id: change.id,
                at,
                failed: attempt_failed
                    || failure.is_some()
                    || rolled.contains(&(change.id, group.to_string())),
                failure,
            });
        }
    }
    out
}

pub(super) fn change_failure_rate(ctx: &Ctx) -> MetricResult {
    let samples = deployed_changes(ctx)
        .into_iter()
        .map(|deployed| Sample {
            at: deployed.at,
            value: if deployed.failed { 100.0 } else { 0.0 },
            change_id: deployed.change_id,
        })
        .collect();
    summarize(
        ctx,
        Spec::percent(MetricId::ChangeFailureRate).windowed(),
        samples,
        copy::no_observation_window(),
    )
}

fn recovered_after(ctx: &Ctx, failure: &ObservedTransitionRow) -> Option<TimeDelta> {
    let (_, window_end) = ctx.window_at(&failure.pipeline_id, failure.at)?;
    ctx.transitions
        .get(&failure.pipeline_id)?
        .iter()
        .find(|row| row.at > failure.at && row.status_after == PipelineStatus::Success.as_str())
        .filter(|row| row.at <= window_end)
        .map(|row| row.at - failure.at)
}

pub(super) fn recovery_time(ctx: &Ctx) -> MetricResult {
    let samples = deployed_changes(ctx)
        .into_iter()
        .filter_map(|deployed| {
            let failure = deployed.failure?;
            Some(Sample {
                at: failure.at,
                value: recovered_after(ctx, failure)?.num_seconds() as f64,
                change_id: deployed.change_id,
            })
        })
        .collect();
    let reason = if ctx.window_since().is_none() {
        copy::no_observation_window()
    } else {
        copy::no_failures_observed()
    };
    summarize(
        ctx,
        Spec::seconds(MetricId::RecoveryTime).windowed(),
        samples,
        reason,
    )
}
