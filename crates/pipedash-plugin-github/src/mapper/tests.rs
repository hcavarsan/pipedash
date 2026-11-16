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

fn run(head_sha: &str) -> types::Run {
    let url = "https://github.test/acme/app";
    serde_json::from_value(serde_json::json!({
        "id": 9001, "workflow_id": 42, "node_id": "n", "name": "CI",
        "head_branch": "main", "head_sha": head_sha, "run_number": 7, "event": "push",
        "status": "completed", "conclusion": "success",
        "created_at": "2026-10-20T10:00:00Z", "updated_at": "2026-10-20T10:10:00Z",
        "url": url, "html_url": url, "jobs_url": url, "logs_url": url,
        "check_suite_url": url, "artifacts_url": url, "cancel_url": url,
        "rerun_url": url, "workflow_url": url,
        "head_commit": {
            "id": head_sha, "tree_id": "def", "message": "ship it",
            "timestamp": "2026-10-20T09:59:00Z",
            "author": {"name": "Ada", "email": "ada@example.com"},
            "committer": {"name": "Ada", "email": "ada@example.com"}
        },
        "repository": {"id": 1, "name": "app", "url": url}
    }))
    .unwrap()
}

#[test]
fn run_source_is_the_pipeline_repository_at_the_head_commit() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let repo = RepoId::from_host_path("github.test", "acme/app");

    let mapped = run_to_pipeline_run(run(sha), "acme__app__42", repo.clone(), Vec::new());

    assert_eq!(
        mapped.correlation,
        Correlation::single_source(repo, CommitSha::parse(sha))
    );
    assert_eq!(
        mapped.correlation.sources[0]
            .commit
            .as_ref()
            .map(CommitSha::as_str),
        Some(sha)
    );
    assert_eq!(mapped.evidence, RunEvidence::default());
}

#[test]
fn a_head_sha_that_is_not_a_commit_leaves_the_source_commit_empty() {
    let repo = RepoId::from_host_path("github.test", "acme/app");

    let mapped = run_to_pipeline_run(run("abc"), "acme__app__42", repo.clone(), Vec::new());

    assert_eq!(mapped.correlation, Correlation::single_source(repo, None));
    assert_eq!(mapped.commit_sha.as_deref(), Some("abc"));
}

#[test]
fn a_run_without_a_repository_has_no_sources() {
    let sha = "0123456789abcdef0123456789abcdef01234567";

    let mapped = run_to_pipeline_run(run(sha), "acme__app__42", None, Vec::new());

    assert_eq!(mapped.correlation, Correlation::default());
}
