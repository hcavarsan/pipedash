use chrono::DateTime;
use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchRunStatus,
};

use super::{
    validate_watch_selection,
    watch_snapshot,
};
use crate::application::services::pipeline_service::WatchedRun;
use crate::application::test_support::{
    pipeline,
    stub_run,
};
use crate::domain::{
    DomainError,
    PipelineRun,
    PipelineStatus,
};

const PIPELINE_ID: &str = "stub__1__Build";

fn run(status: PipelineStatus) -> PipelineRun {
    let mut run = stub_run(PIPELINE_ID, 7, 0);
    run.status = status;
    run.branch = Some("main".to_string());
    run.logs_url = "https://example.com/private?token=secret".to_string();
    run.started_at = DateTime::from_timestamp(100, 0).unwrap();
    run
}

fn watched(run: PipelineRun) -> WatchedRun {
    WatchedRun {
        pipeline: pipeline(1, "Build", PipelineStatus::Success),
        provider_name: "My Stub".to_string(),
        run,
        observed_at: DateTime::from_timestamp(110, 0).unwrap(),
    }
}

fn snapshot(
    run: PipelineRun,
) -> Result<pipedash_plugin_api::build_watch::BuildWatchSnapshot, DomainError> {
    watch_snapshot("session", PIPELINE_ID, 7, watched(run))
}

#[test]
fn selected_run_validation_rejects_unsafe_numbers_and_empty_identity() {
    for run in [i64::MIN, -1, 0, 9_007_199_254_740_992, i64::MAX] {
        assert!(validate_watch_selection("pipeline", run).is_err());
    }
    assert!(validate_watch_selection("", 1).is_err());
    assert!(validate_watch_selection("  ", 1).is_err());
    assert!(validate_watch_selection("pipeline\n", 1).is_err());
    assert!(validate_watch_selection(&"p".repeat(2049), 1).is_err());
    assert!(validate_watch_selection("pipeline", 9_007_199_254_740_991).is_ok());
    assert!(matches!(
        validate_watch_selection("", 1),
        Err(DomainError::InvalidConfig(message))
            if message == "Choose a valid pipeline and a positive, safe run number"
    ));
}

#[test]
fn a_provider_response_must_match_the_selected_run() {
    let mut wrong_pipeline = run(PipelineStatus::Running);
    wrong_pipeline.pipeline_id = "another-pipeline".to_string();
    assert!(snapshot(wrong_pipeline).is_err());
    let mut wrong_run = run(PipelineStatus::Running);
    wrong_run.run_number = 8;
    assert!(snapshot(wrong_run).is_err());
    let mut wrong_selection = watched(run(PipelineStatus::Running));
    wrong_selection.pipeline.id = "another-pipeline".to_string();
    wrong_selection.run.pipeline_id = wrong_selection.pipeline.id.clone();
    assert!(matches!(
        watch_snapshot("session", PIPELINE_ID, 7, wrong_selection),
        Err(DomainError::InvalidConfig(message))
            if message == "The provider returned a different pipeline or build run"
    ));
}

#[test]
fn snapshots_only_include_selected_provider_data_and_never_credentials() {
    let snapshot = snapshot(run(PipelineStatus::Running)).unwrap();
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(snapshot.session_id, "session");
    assert_eq!(snapshot.pipeline_name, "Build");
    assert_eq!(snapshot.provider_name, "My Stub");
    assert_eq!(snapshot.provider_type, "stub");
    assert_eq!(snapshot.status, BuildWatchRunStatus::Running);
    assert_eq!(snapshot.monitoring, BuildWatchMonitoring::Watching);
    assert_eq!(snapshot.branch.as_deref(), Some("main"));
    assert_eq!(snapshot.started_at, 100);
    assert_eq!(snapshot.updated_at, 110);
    assert!(!json.contains("secret"));
    assert!(!json.contains("logs"));
}

#[test]
fn provider_statuses_map_to_watch_statuses() {
    for (status, expected) in [
        (PipelineStatus::Pending, BuildWatchRunStatus::Pending),
        (PipelineStatus::Running, BuildWatchRunStatus::Running),
        (PipelineStatus::Success, BuildWatchRunStatus::Success),
        (PipelineStatus::Failed, BuildWatchRunStatus::Failed),
        (PipelineStatus::Degraded, BuildWatchRunStatus::Failed),
        (PipelineStatus::Cancelled, BuildWatchRunStatus::Cancelled),
        (PipelineStatus::Skipped, BuildWatchRunStatus::Skipped),
    ] {
        assert_eq!(snapshot(run(status)).unwrap().status, expected);
    }
}

#[test]
fn every_terminal_provider_outcome_ends_monitoring_without_fabricating_a_finish_time() {
    for outcome in [
        PipelineStatus::Success,
        PipelineStatus::Failed,
        PipelineStatus::Degraded,
        PipelineStatus::Cancelled,
        PipelineStatus::Skipped,
    ] {
        let snapshot = snapshot(run(outcome)).unwrap();
        assert_eq!(snapshot.monitoring, BuildWatchMonitoring::Completed);
        assert_eq!(snapshot.concluded_at, None);
    }
    for active in [PipelineStatus::Pending, PipelineStatus::Running] {
        let snapshot = snapshot(run(active)).unwrap();
        assert_eq!(snapshot.monitoring, BuildWatchMonitoring::Watching);
    }
}

#[test]
fn a_terminal_snapshot_keeps_the_providers_real_finish_time() {
    let mut completed = run(PipelineStatus::Success);
    completed.concluded_at = DateTime::from_timestamp(105, 0);
    assert_eq!(snapshot(completed).unwrap().concluded_at, Some(105));
}

#[test]
fn an_oversized_snapshot_is_rejected_with_the_watch_limit_message() {
    let mut oversized = watched(run(PipelineStatus::Running));
    oversized.pipeline.id = "a".repeat(1600);
    oversized.run.pipeline_id = oversized.pipeline.id.clone();
    oversized.run.branch = Some("b".repeat(1024));
    let selected = oversized.pipeline.id.clone();
    assert!(matches!(
        watch_snapshot("session", &selected, 7, oversized),
        Err(DomainError::InvalidConfig(message))
            if message == "The build details exceed the watch size limit"
    ));
}
