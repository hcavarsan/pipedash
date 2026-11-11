use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::utils::{
    duration_seconds,
    status_from_conclusion,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};

use crate::{
    config,
    types,
};

pub(crate) fn map_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    status_from_conclusion(status, conclusion, &["success"], &["cancelled"])
}

fn timestamp(value: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    value.filter(|t| *t > DateTime::UNIX_EPOCH)
}

pub(crate) fn map_available_pipeline(repo: &types::Repository) -> AvailablePipeline {
    AvailablePipeline {
        id: repo.full_name.clone(),
        name: repo.full_name.clone(),
        description: Some(repo.description.clone()).filter(|d| !d.is_empty()),
        organization: Some(repo.owner.login.clone()),
        repository: Some(repo.name.clone()),
    }
}

pub(crate) fn map_pipeline(
    owner: &str, repo: &str, workflow: &types::Workflow, latest_run: Option<&types::Run>,
) -> Pipeline {
    Pipeline {
        id: config::build_pipeline_id(owner, repo, &workflow.id),
        name: workflow.name.clone(),
        status: latest_run
            .map(|run| map_status(&run.status, run.conclusion.as_deref()))
            .unwrap_or(PipelineStatus::Pending),
        last_run: latest_run.and_then(|run| timestamp(run.started_at)),
        last_updated: Utc::now(),
        repository: format!("{owner}/{repo}"),
        branch: latest_run.map(|run| run.head_branch.clone()),
        workflow_file: Some(workflow.path.clone()),
        resource_state: None,
        metadata: HashMap::new(),
    }
}

pub(crate) fn map_run(run: &types::Run, pipeline_id: &str, owner: &str) -> PipelineRun {
    let started_at = timestamp(run.started_at);
    let concluded_at = timestamp(run.completed_at);

    PipelineRun {
        id: format!("gitea-run-{}", run.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: run.run_number,
        status: map_status(&run.status, run.conclusion.as_deref()),
        started_at: started_at.unwrap_or_else(Utc::now),
        concluded_at,
        duration_seconds: duration_seconds(started_at, concluded_at),
        logs_url: run.html_url.clone(),
        commit_sha: Some(run.head_sha.clone()),
        commit_message: Some(run.display_title.clone()),
        branch: Some(run.head_branch.clone()),
        actor: run.trigger_actor.as_ref().map(|actor| actor.login.clone()),
        inputs: Some(serde_json::json!({ "ref": run.head_branch })),
        metadata: HashMap::from([
            ("event".to_string(), serde_json::json!(run.event)),
            ("run_id".to_string(), serde_json::json!(run.id)),
            ("owner".to_string(), serde_json::json!(owner)),
        ]),
    }
}

fn job_log_availability(
    status: &str, started_at: Option<DateTime<Utc>>,
) -> Option<LogAvailability> {
    match status {
        "in_progress" => Some(LogAvailability::Partial),
        "completed" => started_at.map(|_| LogAvailability::Ready),
        _ => Some(LogAvailability::Pending),
    }
}

pub(crate) fn jobs_to_log_nodes(jobs: &[types::Job]) -> Vec<LogNode> {
    let step_count = jobs
        .iter()
        .map(|job| job.steps.as_ref().map_or(0, Vec::len));
    let mut nodes = Vec::with_capacity(jobs.len() + step_count.sum::<usize>());

    for job in jobs {
        let job_id = job.id.to_string();
        let started_at = timestamp(job.started_at);

        nodes.push(LogNode {
            id: job_id.clone(),
            parent_id: None,
            kind: LogNodeKind::Job,
            name: job.name.clone(),
            status: map_status(&job.status, job.conclusion.as_deref()),
            started_at,
            finished_at: timestamp(job.completed_at),
            log: job_log_availability(&job.status, started_at),
            web_url: Some(job.html_url.clone()).filter(|url| !url.is_empty()),
        });

        for step in job.steps.iter().flatten() {
            nodes.push(LogNode::step(
                &job_id,
                step.number,
                step.name.clone(),
                map_status(&step.status, step.conclusion.as_deref()),
                timestamp(step.started_at),
                timestamp(step.completed_at),
            ));
        }
    }

    nodes
}

#[cfg(test)]
mod tests;
