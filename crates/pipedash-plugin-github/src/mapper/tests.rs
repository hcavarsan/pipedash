use super::*;

#[test]
fn every_documented_conclusion_maps_to_a_finished_status() {
    let cases = [
        ("success", PipelineStatus::Success),
        ("neutral", PipelineStatus::Success),
        ("failure", PipelineStatus::Failed),
        ("timed_out", PipelineStatus::Failed),
        ("startup_failure", PipelineStatus::Failed),
        ("action_required", PipelineStatus::Failed),
        ("cancelled", PipelineStatus::Cancelled),
        ("stale", PipelineStatus::Cancelled),
        ("skipped", PipelineStatus::Skipped),
    ];
    for (conclusion, expected) in cases {
        assert_eq!(
            map_status("completed", Some(conclusion)),
            expected,
            "conclusion {conclusion}"
        );
    }
}

#[test]
fn unfinished_statuses_are_running_or_pending() {
    assert_eq!(map_status("in_progress", None), PipelineStatus::Running);
    assert_eq!(map_status("queued", None), PipelineStatus::Running);
    for status in ["requested", "waiting", "pending"] {
        assert_eq!(
            map_status(status, None),
            PipelineStatus::Pending,
            "status {status}"
        );
    }
}

#[test]
fn every_documented_workflow_state_maps_to_its_disabled_reason() {
    let cases = [
        ("active", None),
        ("deleted", None),
        (
            "disabled_inactivity",
            Some("GitHub disabled this workflow after 60 days without repository activity"),
        ),
        (
            "disabled_manually",
            Some("Someone disabled this workflow on GitHub"),
        ),
        (
            "disabled_fork",
            Some("GitHub disables workflows in a fork until you enable them"),
        ),
    ];
    for (workflow_state, reason) in cases {
        assert_eq!(
            disabled_state(workflow_state),
            reason.map(|reason| ResourceState::new("Disabled", Some(reason))),
            "state {workflow_state}"
        );
    }
}
