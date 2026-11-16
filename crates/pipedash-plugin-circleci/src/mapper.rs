use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::utils::duration_seconds;
use pipedash_plugin_api::{
    AvailablePipeline,
    CommitSha,
    Correlation,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RepoId,
    RunEvidence,
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
        correlation: Correlation::single_source(
            vcs.and_then(|vcs| vcs.target_repository_url.as_deref())
                .and_then(RepoId::parse),
            vcs.and_then(|vcs| vcs.revision.as_deref())
                .and_then(CommitSha::parse),
        ),
        evidence: RunEvidence::default(),
        id: format!("{pipeline_id}__{}", pipeline.id),
        pipeline_id: pipeline_id.into(),
        run_number: pipeline.number,
        status,
        started_at,
        concluded_at,
        duration_seconds: duration_seconds(Some(started_at), concluded_at)
            .map(|seconds| seconds.max(0)),
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

pub fn to_pipeline(project: &Project, latest: Option<&PipelineRun>) -> Pipeline {
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
        correlation: Correlation::single_source(RepoId::parse(&project.repository_url), None),
        id: project.slug.clone(),
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
mod tests;
