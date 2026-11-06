use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};

use crate::config;
use crate::types::{
    CirclePipeline,
    Job,
    Project,
    Workflow,
};

pub(crate) fn pipeline_url(base_url: &str, project_slug: &str, number: i64) -> String {
    let base_url = match base_url.trim_end_matches('/') {
        config::DEFAULT_BASE_URL => "https://app.circleci.com",
        other => other,
    };
    let (vcs_slug, project) = project_slug.split_once('/').unwrap_or(("", project_slug));
    let vcs_slug = match vcs_slug {
        "gh" => "github",
        "bb" => "bitbucket",
        other => other,
    };
    format!("{base_url}/pipelines/{vcs_slug}/{project}/{number}")
}

pub(crate) fn job_status(status: &str) -> PipelineStatus {
    match status {
        "success" => PipelineStatus::Success,
        "failed" | "infrastructure_fail" | "timedout" | "terminated-unknown" | "unauthorized" => {
            PipelineStatus::Failed
        }
        "running" => PipelineStatus::Running,
        "canceled" => PipelineStatus::Cancelled,
        "not_run" | "retried" => PipelineStatus::Skipped,
        _ => PipelineStatus::Pending,
    }
}

fn job_log(job: &Job) -> Option<LogAvailability> {
    job.job_number?;
    if job.job_type.as_deref() == Some("approval") {
        return None;
    }
    match job_status(&job.status) {
        PipelineStatus::Running => Some(LogAvailability::Partial),
        PipelineStatus::Pending => Some(LogAvailability::Pending),
        PipelineStatus::Skipped => None,
        _ => job.started_at.map(|_| LogAvailability::Ready),
    }
}

pub(crate) fn log_nodes(
    base_url: &str, project_slug: &str, number: i64, workflows: &[(&Workflow, Vec<Job>)],
) -> Vec<LogNode> {
    let run_url = pipeline_url(base_url, project_slug, number);
    let mut nodes = Vec::new();
    for (workflow, jobs) in workflows {
        let workflow_id = format!("workflow:{}", workflow.id);
        let workflow_url = format!("{run_url}/workflows/{}", workflow.id);
        nodes.push(LogNode {
            id: workflow_id.clone(),
            parent_id: None,
            kind: LogNodeKind::Stage,
            name: workflow.name.clone(),
            status: workflow_status(&workflow.status),
            started_at: workflow.started_at.or(workflow.created_at),
            finished_at: workflow.stopped_at.filter(|_| !is_active(&workflow.status)),
            log: None,
            web_url: Some(workflow_url.clone()),
        });
        nodes.extend(jobs.iter().map(|job| LogNode {
            id: match (job.job_number, &job.id) {
                (Some(number), _) => number.to_string(),
                (None, Some(id)) => format!("job:{id}"),
                (None, None) => format!("job:{}:{}", workflow.id, job.name),
            },
            parent_id: Some(workflow_id.clone()),
            kind: LogNodeKind::Job,
            name: job.name.clone(),
            status: job_status(&job.status),
            started_at: job.started_at,
            finished_at: job.stopped_at,
            log: job_log(job),
            web_url: Some(match job.job_number {
                Some(number) => format!("{workflow_url}/jobs/{number}"),
                None => workflow_url.clone(),
            }),
        }));
    }
    nodes
}

pub fn is_active(status: &str) -> bool {
    matches!(
        status,
        "running" | "failing" | "on_hold" | "pending" | "queued"
    )
}

fn workflow_status(status: &str) -> PipelineStatus {
    match status {
        "success" => PipelineStatus::Success,
        "failed" | "error" | "unauthorized" => PipelineStatus::Failed,
        "running" | "failing" => PipelineStatus::Running,
        "canceled" => PipelineStatus::Cancelled,
        "not_run" => PipelineStatus::Skipped,
        _ => PipelineStatus::Pending,
    }
}

pub fn latest_workflows(workflows: &[Workflow]) -> Vec<&Workflow> {
    let mut latest: HashMap<&str, &Workflow> = HashMap::new();
    for workflow in workflows {
        let entry = latest.entry(&workflow.name).or_insert(workflow);
        if workflow.created_at > entry.created_at {
            *entry = workflow;
        }
    }
    let mut latest: Vec<_> = latest.into_values().collect();
    latest.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    latest
}

fn aggregate_status(pipeline: &CirclePipeline, workflows: &[&Workflow]) -> PipelineStatus {
    if pipeline.state == "errored" || !pipeline.errors.is_empty() {
        return PipelineStatus::Failed;
    }
    if pipeline.state == "setup" {
        return PipelineStatus::Running;
    }
    if matches!(pipeline.state.as_str(), "pending" | "setup-pending") || workflows.is_empty() {
        return PipelineStatus::Pending;
    }
    workflows
        .iter()
        .map(|workflow| workflow_status(&workflow.status))
        .max_by_key(|status| match status {
            PipelineStatus::Running => 6,
            PipelineStatus::Pending => 5,
            PipelineStatus::Failed => 4,
            PipelineStatus::Cancelled => 3,
            PipelineStatus::Success => 2,
            _ => 1,
        })
        .unwrap_or(PipelineStatus::Pending)
}

pub fn to_run(
    pipeline_id: &str, pipeline: &CirclePipeline, workflows: &[Workflow], base_url: &str,
    project_slug: &str,
) -> PipelineRun {
    let workflows = latest_workflows(workflows);
    let status = aggregate_status(pipeline, &workflows);
    let started_at = workflows
        .iter()
        .filter_map(|workflow| workflow.started_at.or(workflow.created_at))
        .min()
        .unwrap_or(pipeline.created_at);
    let concluded_at = if matches!(status, PipelineStatus::Running | PipelineStatus::Pending) {
        None
    } else {
        workflows
            .iter()
            .filter_map(|workflow| workflow.stopped_at)
            .max()
            .or(pipeline.updated_at)
    };
    let vcs = pipeline.vcs.as_ref();
    let metadata = HashMap::from([
        (
            "circleci_pipeline_id".into(),
            serde_json::json!(pipeline.id),
        ),
        (
            "workflow_ids".into(),
            serde_json::json!(workflows.iter().map(|w| &w.id).collect::<Vec<_>>()),
        ),
        (
            "can_rerun".into(),
            serde_json::json!(
                !workflows.is_empty()
                    && !workflows.iter().any(|workflow| is_active(&workflow.status))
            ),
        ),
        (
            "can_rerun_failed_jobs".into(),
            serde_json::json!(
                workflows
                    .iter()
                    .any(|workflow| matches!(workflow.status.as_str(), "failed" | "error"))
                    && !workflows.iter().any(|workflow| is_active(&workflow.status))
            ),
        ),
    ]);
    PipelineRun {
        id: format!("{pipeline_id}__{}", pipeline.id),
        pipeline_id: pipeline_id.into(),
        run_number: pipeline.number,
        status,
        started_at,
        concluded_at,
        duration_seconds: concluded_at.map(|end| (end - started_at).num_seconds().max(0)),
        logs_url: pipeline_url(base_url, project_slug, pipeline.number),
        commit_sha: vcs.and_then(|vcs| vcs.revision.clone()),
        commit_message: vcs
            .and_then(|vcs| vcs.commit.as_ref())
            .and_then(|commit| commit.subject.clone()),
        branch: vcs.and_then(|vcs| vcs.branch.clone()),
        actor: pipeline
            .trigger
            .as_ref()
            .and_then(|trigger| trigger.actor.as_ref())
            .and_then(|actor| actor.login.clone()),
        inputs: None,
        metadata,
    }
}

pub fn to_pipeline(provider_id: i64, project: &Project, latest: Option<&PipelineRun>) -> Pipeline {
    let mut metadata = HashMap::from([("project_slug".into(), serde_json::json!(project.slug))]);
    if let Some(branch) = project.default_branch.as_deref().filter(|b| !b.is_empty()) {
        metadata.insert("default_branch".into(), serde_json::json!(branch));
    }
    if !project.repository_url.is_empty() {
        metadata.insert(
            "repository_url".into(),
            serde_json::json!(project.repository_url),
        );
    }
    Pipeline {
        id: config::build_pipeline_id(provider_id, &project.slug),
        provider_id,
        provider_type: "circleci".into(),
        name: project.name.clone(),
        status: latest.map_or(PipelineStatus::Pending, |run| run.status),
        last_run: latest.map(|run| run.started_at),
        last_updated: Utc::now(),
        repository: repository(project),
        branch: latest
            .and_then(|run| run.branch.clone())
            .or_else(|| project.default_branch.clone()),
        workflow_file: None,
        resource_state: None,
        metadata,
    }
}

fn repository(project: &Project) -> String {
    if project.organization.is_empty() {
        project.name.clone()
    } else {
        format!("{}/{}", project.organization, project.name)
    }
}

pub(crate) fn to_available_pipeline(project: &Project) -> AvailablePipeline {
    let mut details = vec![format!("Project: {}", project.slug)];
    if let Some(branch) = project.default_branch.as_deref().filter(|b| !b.is_empty()) {
        details.push(format!("Default branch: {branch}"));
    }
    AvailablePipeline {
        id: project.slug.clone(),
        name: project.name.clone(),
        description: Some(details.join(" | ")),
        organization: Some(project.organization.clone()).filter(|org| !org.is_empty()),
        repository: Some(repository(project)),
    }
}

#[cfg(test)]
mod tests {
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
                "circleci__1__gh/org/repo",
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
                "circleci__1__gh/org/repo",
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
            "circleci__1__gh/org/repo",
            &pipeline("created"),
            &[workflow("success")],
            "https://circleci.com",
            "gh/org/repo",
        );
        assert_eq!(run.id, "circleci__1__gh/org/repo__pipeline-id");
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
        let mapped = to_pipeline(1, &project, Some(&run));
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
}
