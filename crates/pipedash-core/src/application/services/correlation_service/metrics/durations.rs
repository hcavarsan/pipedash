use chrono::{
    DateTime,
    Utc,
};

use super::context::{
    Ctx,
    Journey,
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
use crate::application::services::correlation_service::copy;
use crate::domain::application::metrics::{
    MetricId,
    MetricResult,
};

fn seconds(from: DateTime<Utc>, to: DateTime<Utc>) -> Option<f64> {
    let delta = (to - from).num_seconds();
    (delta >= 0).then_some(delta as f64)
}

fn over_journeys(
    ctx: &Ctx, journeys: &[Journey<'_>], spec: Spec, reason: String,
    value: impl Fn(&Journey<'_>) -> Option<f64>,
) -> MetricResult {
    let reason = if journeys.is_empty() {
        copy::no_deployments()
    } else {
        reason
    };
    let samples = journeys
        .iter()
        .filter_map(|journey| {
            Some(Sample {
                at: journey.deployed_at,
                value: value(journey)?,
                change_id: journey.change.id,
            })
        })
        .collect();
    summarize(ctx, spec, samples, reason)
}

fn first_build_start(change: &ChangeData) -> Option<DateTime<Utc>> {
    change
        .builds
        .iter()
        .filter(|run| run.timing_recorded)
        .map(|run| run.started_at)
        .min()
}

fn recorded_end(run: &StageRun) -> Option<DateTime<Utc>> {
    run.timed_span().map(|(_, ended)| ended)
}

fn built_at(change: &ChangeData) -> Option<DateTime<Utc>> {
    change.first_green_build().and_then(recorded_end)
}

fn upstream_end(ctx: &Ctx, journey: &Journey<'_>) -> Option<DateTime<Utc>> {
    let upstream = ctx.upstream_groups(journey.group);
    if upstream.is_empty() {
        return built_at(journey.change);
    }
    upstream
        .into_iter()
        .filter_map(|group| journey.change.first_deploy(group))
        .filter_map(recorded_end)
        .max()
}

pub(super) fn lead_time(ctx: &Ctx, journeys: &[Journey<'_>]) -> MetricResult {
    over_journeys(
        ctx,
        journeys,
        Spec::seconds(MetricId::LeadTime).spread(),
        copy::no_commit_times(),
        |journey| seconds(journey.change.authored_at?, journey.deployed_at),
    )
}

pub(super) fn delivery_time(ctx: &Ctx, journeys: &[Journey<'_>]) -> MetricResult {
    over_journeys(
        ctx,
        journeys,
        Spec::seconds(MetricId::DeliveryTime).spread(),
        copy::no_recorded_timing(),
        |journey| seconds(first_build_start(journey.change)?, journey.deployed_at),
    )
}

pub(super) fn build_time(ctx: &Ctx) -> MetricResult {
    let samples = ctx
        .changes
        .iter()
        .filter_map(|change| {
            let (started, ended) = change.first_green_build()?.timed_span()?;
            Some(Sample {
                at: ended,
                value: seconds(started, ended)?,
                change_id: change.id,
            })
        })
        .collect();
    summarize(
        ctx,
        Spec::seconds(MetricId::BuildTime),
        samples,
        copy::no_recorded_timing(),
    )
}

pub(super) fn wait_time(ctx: &Ctx, journeys: &[Journey<'_>]) -> MetricResult {
    over_journeys(
        ctx,
        journeys,
        Spec::seconds(MetricId::WaitTime),
        copy::no_recorded_start(),
        |journey| {
            let deploy = journey.deploy;
            if !(deploy.timing_recorded && deploy.attempt) {
                return None;
            }
            seconds(upstream_end(ctx, journey)?, deploy.started_at)
        },
    )
}

pub(super) fn build_to_deployed(ctx: &Ctx, journeys: &[Journey<'_>]) -> MetricResult {
    over_journeys(
        ctx,
        journeys,
        Spec::seconds(MetricId::BuildToDeployed),
        copy::no_recorded_timing(),
        |journey| seconds(built_at(journey.change)?, journey.deployed_at),
    )
}

pub(super) fn deploy_time(ctx: &Ctx, journeys: &[Journey<'_>]) -> MetricResult {
    let reason = if journeys.iter().all(|journey| !journey.deploy.attempt) {
        copy::no_attempt_records()
    } else {
        copy::no_recorded_timing()
    };
    over_journeys(
        ctx,
        journeys,
        Spec::seconds(MetricId::DeployTime),
        reason,
        |journey| {
            let deploy = journey.deploy;
            if !deploy.attempt {
                return None;
            }
            let (started, ended) = deploy.timed_span()?;
            seconds(started, ended)
        },
    )
}
