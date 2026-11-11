use serde_json::json;

use super::*;

fn pipeline(state: &str) -> CirclePipeline {
    serde_json::from_value(json!({
        "id": "pipeline-id", "number": 42, "state": state,
        "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:01:00Z",
        "vcs": {"branch": "main", "revision": "abc", "commit": {"subject": "Fix"}},
        "trigger": {"actor": {"login": "dev"}}
    }))
    .unwrap()
}

fn workflow(status: &str) -> Workflow {
    serde_json::from_value(json!({
        "id": status, "name": status, "status": status,
        "created_at": "2026-01-01T00:00:10Z", "stopped_at": "2026-01-01T00:00:50Z"
    }))
    .unwrap()
}

fn aggregate(pipeline: &CirclePipeline, workflows: &[Workflow]) -> PipelineStatus {
    aggregate_status(pipeline, &latest_workflows(workflows))
}

#[test]
fn successful_reruns_replace_failures_without_hiding_independent_workflows() {
    let mut failed = workflow("failed");
    failed.name = "build".into();
    let mut success = workflow("success");
    success.name = "build".into();
    success.created_at = Some(failed.created_at.unwrap() + chrono::Duration::seconds(60));
    let attempts = [success.clone(), failed.clone()];
    let latest = latest_workflows(&attempts);
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].id, "success");
    assert_eq!(
        aggregate(&pipeline("created"), &attempts),
        PipelineStatus::Success
    );
    let independent = workflow("failed");
    let attempts = [failed, success, independent];
    assert_eq!(
        aggregate(&pipeline("created"), &attempts),
        PipelineStatus::Failed
    );
    let run = to_run(
        "id",
        &pipeline("created"),
        &attempts,
        "https://circleci.com",
        "gh/org/repo",
    );
    assert_eq!(run.metadata["can_rerun_failed_jobs"], true);
}

#[test]
fn documented_workflow_statuses_map_without_losing_active_runs() {
    for (status, expected, active) in [
        ("success", PipelineStatus::Success, false),
        ("running", PipelineStatus::Running, true),
        ("not_run", PipelineStatus::Skipped, false),
        ("failed", PipelineStatus::Failed, false),
        ("error", PipelineStatus::Failed, false),
        ("failing", PipelineStatus::Running, true),
        ("on_hold", PipelineStatus::Pending, true),
        ("canceled", PipelineStatus::Cancelled, false),
        ("unauthorized", PipelineStatus::Failed, false),
    ] {
        assert_eq!(
            aggregate(&pipeline("created"), &[workflow(status)]),
            expected,
            "{status}"
        );
        assert_eq!(is_active(status), active, "{status}");
        let run = to_run(
            "gh/org/repo",
            &pipeline("created"),
            &[workflow(status)],
            "https://circleci.com",
            "gh/org/repo",
        );
        assert_eq!(
            run.metadata["can_rerun_failed_jobs"],
            matches!(status, "failed" | "error"),
            "{status}"
        );
    }
}

#[test]
fn all_workflows_affect_status_and_completion() {
    for (statuses, expected) in [
        (["success", "failed"], PipelineStatus::Failed),
        (["failed", "running"], PipelineStatus::Running),
        (["success", "on_hold"], PipelineStatus::Pending),
        (["success", "canceled"], PipelineStatus::Cancelled),
        (["success", "not_run"], PipelineStatus::Success),
    ] {
        let workflows = statuses.map(workflow);
        let run = to_run(
            "gh/org/repo",
            &pipeline("created"),
            &workflows,
            "https://circleci.com/",
            "gh/org/repo",
        );
        assert_eq!(run.status, expected);
        assert_eq!(
            run.concluded_at.is_none(),
            matches!(expected, PipelineStatus::Running | PipelineStatus::Pending)
        );
    }
}

#[test]
fn pipeline_errors_and_setup_survive_missing_workflows() {
    for (state, expected) in [
        ("errored", PipelineStatus::Failed),
        ("created", PipelineStatus::Pending),
        ("pending", PipelineStatus::Pending),
        ("setup-pending", PipelineStatus::Pending),
        ("setup", PipelineStatus::Running),
    ] {
        assert_eq!(aggregate(&pipeline(state), &[]), expected, "{state}");
    }
    assert_eq!(
        aggregate(&pipeline("setup"), &[workflow("success")]),
        PipelineStatus::Running
    );
    let mut errored = pipeline("created");
    errored
        .errors
        .push(json!({"type": "config", "message": "Invalid config"}));
    assert_eq!(aggregate(&errored, &[]), PipelineStatus::Failed);
}

#[test]
fn run_keeps_metadata_timing_and_circleci_link() {
    let run = to_run(
        "gh/org/repo",
        &pipeline("created"),
        &[workflow("success")],
        "https://circleci.com",
        "gh/org/repo",
    );
    assert_eq!(run.id, "gh/org/repo__pipeline-id");
    assert_eq!(run.duration_seconds, Some(40));
    assert_eq!(
        run.logs_url,
        "https://app.circleci.com/pipelines/github/org/repo/42"
    );
    assert_eq!(run.commit_sha.as_deref(), Some("abc"));
    assert_eq!(run.commit_message.as_deref(), Some("Fix"));
    assert_eq!(run.actor.as_deref(), Some("dev"));
    assert_eq!(run.branch.as_deref(), Some("main"));
    assert_eq!(run.metadata["circleci_pipeline_id"], "pipeline-id");
    let project = Project {
        slug: "gh/org/repo".into(),
        id: None,
        name: "repo".into(),
        organization: "org".into(),
        repository_url: "https://github.com/org/repo".into(),
        default_branch: Some("main".into()),
    };
    let mapped = to_pipeline(&project, Some(&run));
    assert_eq!(mapped.id, run.pipeline_id);
    assert_eq!(mapped.status, PipelineStatus::Success);
    assert_eq!(mapped.repository, "org/repo");
    assert_eq!(
        pipeline_url("https://circleci.acme.dev/", "circleci/o/p", 3),
        "https://circleci.acme.dev/pipelines/circleci/o/p/3"
    );
}

fn job(number: Option<i64>, status: &str, kind: &str, started: bool) -> Job {
    serde_json::from_value(json!({
        "id": format!("job-{status}"), "name": status, "job_number": number,
        "status": status, "type": kind,
        "started_at": started.then_some("2026-01-01T00:00:10Z"),
    }))
    .unwrap()
}

#[test]
fn workflows_hold_their_jobs_and_only_jobs_with_numbers_have_logs() {
    let build = workflow("failed");
    let jobs = vec![
        job(Some(11), "success", "build", true),
        job(Some(12), "running", "build", true),
        job(Some(13), "queued", "build", false),
        job(Some(14), "canceled", "build", false),
        job(Some(15), "infrastructure_fail", "build", true),
        job(None, "on_hold", "approval", false),
        job(Some(16), "not_run", "build", false),
    ];
    let nodes = log_nodes("https://circleci.com", "gh/org/repo", 42, &[(&build, jobs)]);

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.parent_id.as_deref(),
                node.status,
                node.log,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("workflow:failed", None, PipelineStatus::Failed, None),
            (
                "11",
                Some("workflow:failed"),
                PipelineStatus::Success,
                Some(LogAvailability::Ready)
            ),
            (
                "12",
                Some("workflow:failed"),
                PipelineStatus::Running,
                Some(LogAvailability::Partial)
            ),
            (
                "13",
                Some("workflow:failed"),
                PipelineStatus::Pending,
                Some(LogAvailability::Pending)
            ),
            (
                "14",
                Some("workflow:failed"),
                PipelineStatus::Cancelled,
                None
            ),
            (
                "15",
                Some("workflow:failed"),
                PipelineStatus::Failed,
                Some(LogAvailability::Ready)
            ),
            (
                "job:job-on_hold",
                Some("workflow:failed"),
                PipelineStatus::Pending,
                None
            ),
            ("16", Some("workflow:failed"), PipelineStatus::Skipped, None),
        ]
    );
    assert_eq!(
        nodes[1].web_url.as_deref(),
        Some("https://app.circleci.com/pipelines/github/org/repo/42/workflows/failed/jobs/11")
    );
}
