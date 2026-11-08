use chrono::{
    Duration,
    Utc,
};

use super::run_pace;
use crate::application::test_support::stub_run;
use crate::domain::{
    PipelineRun,
    PipelineStatus,
};

fn run_started(status: PipelineStatus, seconds_ago: i64) -> (PipelineRun, chrono::DateTime<Utc>) {
    let now = Utc::now();
    let mut run = stub_run("stub__1__build", 1, 0);
    run.status = status;
    run.started_at = now - Duration::seconds(seconds_ago);
    (run, now)
}

#[test]
fn run_pace_should_flag_only_live_runs_past_their_typical_duration() {
    let (running, now) = run_started(PipelineStatus::Running, 700);
    let pace = run_pace(&running, Some(600), now);
    assert!(pace.live && pace.past_usual);
    assert_eq!(pace.typical_duration_seconds, Some(600));

    let (running, now) = run_started(PipelineStatus::Running, 500);
    assert!(!run_pace(&running, Some(600), now).past_usual);

    let (finished, now) = run_started(PipelineStatus::Success, 700);
    let pace = run_pace(&finished, Some(600), now);
    assert!(!pace.live && !pace.past_usual);

    let (running, now) = run_started(PipelineStatus::Running, 700);
    let pace = run_pace(&running, Some(0), now);
    assert!(pace.live && !pace.past_usual);
    assert_eq!(pace.typical_duration_seconds, None);
    assert!(!run_pace(&running, None, now).past_usual);

    let (future, now) = run_started(PipelineStatus::Running, -30);
    assert!(!run_pace(&future, Some(1), now).past_usual);
}
