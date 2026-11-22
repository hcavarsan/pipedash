use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::plugin::defaults::awaits_review;

use crate::domain::{
    PipelineRun,
    PipelineStatus,
    RunPace,
};

pub(crate) fn waits_in_queue(run: &PipelineRun) -> bool {
    run.status == PipelineStatus::Pending && !awaits_review(&run.metadata)
}

pub(crate) fn run_pace(
    run: &PipelineRun, typical_duration_seconds: Option<i64>, queue_limit_seconds: i64,
    now: DateTime<Utc>,
) -> RunPace {
    let live = run.status == PipelineStatus::Running;
    let queued = waits_in_queue(run);
    let typical_duration_seconds = typical_duration_seconds.filter(|typical| *typical > 0);
    let elapsed = (now - run.started_at).num_seconds().max(0);

    RunPace {
        live,
        typical_duration_seconds,
        past_usual: live && typical_duration_seconds.is_some_and(|typical| elapsed > typical),
        queued,
        queue_limit_seconds: queued.then_some(queue_limit_seconds),
        stuck: queued && elapsed > queue_limit_seconds,
    }
}

#[cfg(test)]
mod tests;
