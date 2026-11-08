use chrono::{
    DateTime,
    Utc,
};

use crate::domain::{
    PipelineRun,
    PipelineStatus,
    RunPace,
};

pub(crate) fn run_pace(
    run: &PipelineRun, typical_duration_seconds: Option<i64>, now: DateTime<Utc>,
) -> RunPace {
    let live = run.status == PipelineStatus::Running;
    let typical_duration_seconds = typical_duration_seconds.filter(|typical| *typical > 0);
    let elapsed = (now - run.started_at).num_seconds().max(0);

    RunPace {
        live,
        typical_duration_seconds,
        past_usual: live && typical_duration_seconds.is_some_and(|typical| elapsed > typical),
    }
}

#[cfg(test)]
mod tests;
