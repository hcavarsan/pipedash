use super::context::Ctx;
use super::series::{
    summarize,
    Fold,
    Sample,
    Spec,
};
use crate::application::services::correlation_service::copy;
use crate::domain::application::metrics::{
    FunnelStep,
    MetricId,
    MetricResult,
    MetricUnit,
};

fn percent(hit: bool) -> f64 {
    if hit {
        100.0
    } else {
        0.0
    }
}

pub(super) fn deploy_frequency(ctx: &Ctx) -> MetricResult {
    let samples = ctx
        .changes
        .iter()
        .flat_map(|change| {
            ctx.deploys(change).filter_map(|run| {
                Some(Sample {
                    at: run.succeeded_at()?,
                    value: 1.0,
                    change_id: change.id,
                })
            })
        })
        .collect();
    let spec = Spec {
        id: MetricId::DeployFrequency,
        unit: MetricUnit::Count,
        fold: Fold::Sum,
        spread: false,
        windowed: false,
    };
    summarize(ctx, spec, samples, copy::no_deployments())
}

pub(super) fn build_success_rate(ctx: &Ctx) -> MetricResult {
    let samples = ctx
        .changes
        .iter()
        .flat_map(|change| {
            change
                .builds
                .iter()
                .filter(|run| run.outcome_recorded && run.is_concluded())
                .filter_map(|run| {
                    Some(Sample {
                        at: run.concluded_at?,
                        value: percent(run.is_success()),
                        change_id: change.id,
                    })
                })
        })
        .collect();
    summarize(
        ctx,
        Spec::percent(MetricId::BuildSuccessRate),
        samples,
        copy::no_recorded_outcome(),
    )
}

pub(super) fn deploy_failures(ctx: &Ctx) -> MetricResult {
    let attempts: Vec<_> = ctx
        .changes
        .iter()
        .flat_map(|change| ctx.deploys(change).map(move |run| (change.id, run)))
        .filter(|(_, run)| run.attempt)
        .collect();
    let reason = if attempts.is_empty() {
        copy::no_attempt_records()
    } else {
        copy::no_recorded_outcome()
    };
    let samples = attempts
        .into_iter()
        .filter(|(_, run)| run.outcome_recorded && run.is_concluded())
        .filter_map(|(change_id, run)| {
            Some(Sample {
                at: run.concluded_at?,
                value: percent(run.is_failed()),
                change_id,
            })
        })
        .collect();
    summarize(
        ctx,
        Spec::percent(MetricId::DeployFailures),
        samples,
        reason,
    )
}

pub(super) fn funnel(ctx: &Ctx) -> Vec<FunnelStep> {
    let scoped: Vec<_> = ctx
        .changes
        .iter()
        .filter(|change| ctx.in_scope(change))
        .collect();
    let step = |key: String, label: String, count: usize| FunnelStep { key, label, count };
    let mut steps = vec![
        step(
            "built".into(),
            copy::funnel_built(),
            scoped
                .iter()
                .filter(|change| !change.builds.is_empty())
                .count(),
        ),
        step(
            "passed".into(),
            copy::funnel_passed(),
            scoped
                .iter()
                .filter(|change| change.first_green_build().is_some())
                .count(),
        ),
    ];
    for group in ctx.groups.iter().filter(|g| ctx.group_visible(&g.key)) {
        let count = scoped
            .iter()
            .filter(|change| change.first_deploy(&group.key).is_some())
            .count();
        steps.push(step(
            format!("deployed:{}", group.key),
            copy::funnel_deployed(&group.label),
            count,
        ));
    }
    steps
}
