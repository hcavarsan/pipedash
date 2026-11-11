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
    Observed,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RepoId,
    RunEvidence,
    TargetObservation,
};

use crate::types;

pub(crate) fn map_status(gitlab_status: &str) -> PipelineStatus {
    match gitlab_status {
        "success" => PipelineStatus::Success,
        "failed" => PipelineStatus::Failed,
        "running" | "pending" | "canceling" => PipelineStatus::Running,
        "canceled" => PipelineStatus::Cancelled,
        "skipped" => PipelineStatus::Skipped,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn map_pipeline(
    project: &types::Project, pipeline: Option<&types::Pipeline>,
) -> Pipeline {
    let status = pipeline
        .map(|p| map_status(&p.status))
        .unwrap_or(PipelineStatus::Pending);
    let last_run = pipeline.and_then(|p| p.updated_at.into());
    let branch = pipeline.map(|p| p.ref_name.clone());

    let repository_normalized = project.name_with_namespace.replace(" ", "");

    let parts: Vec<&str> = project.name_with_namespace.split('/').collect();
    let namespace = if parts.len() >= 2 {
        parts[..parts.len() - 1].join("/")
    } else {
        String::new()
    };

    let mut metadata = HashMap::new();
    metadata.insert("project_id".to_string(), serde_json::json!(project.id));
    if !namespace.is_empty() {
        metadata.insert("namespace".to_string(), serde_json::json!(namespace));
    }

    Pipeline {
        correlation: Correlation::single_source(RepoId::parse(&project.web_url), None),
        id: project.id.to_string(),
        name: project.name.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository: repository_normalized,
        branch,
        workflow_file: None,
        resource_state: None,
        metadata,
    }
}

pub(crate) fn map_pipeline_run(
    pipeline: &types::Pipeline, project: &types::Project, namespace: Option<&str>,
    targets: Vec<TargetObservation>,
) -> PipelineRun {
    let duration = pipeline.started_at.and_then(|started| {
        pipeline
            .duration
            .or_else(|| duration_seconds(Some(started), pipeline.finished_at))
    });
    let timing = if pipeline.started_at.is_some() {
        Observed::Recorded
    } else {
        Observed::Absent
    };

    let mut metadata = HashMap::new();
    if let Some(source) = &pipeline.source {
        metadata.insert("source".to_string(), serde_json::json!(source));
    }
    if let Some(ns) = namespace {
        metadata.insert("namespace".to_string(), serde_json::json!(ns));
    }

    PipelineRun {
        correlation: Correlation {
            targets,
            ..Correlation::single_source(
                RepoId::parse(&project.web_url),
                CommitSha::parse(&pipeline.sha),
            )
        },
        evidence: RunEvidence {
            timing,
            ..RunEvidence::default()
        },
        id: format!("{}_{}", project.id, pipeline.id),
        pipeline_id: project.id.to_string(),
        run_number: pipeline.id,
        status: map_status(&pipeline.status),
        started_at: pipeline.started_at.unwrap_or(pipeline.created_at),
        concluded_at: pipeline.finished_at,
        duration_seconds: duration,
        logs_url: pipeline.web_url.clone(),
        commit_sha: Some(pipeline.sha.clone()),
        commit_message: None,
        branch: Some(pipeline.ref_name.clone()),
        actor: pipeline.user.as_ref().map(|u| u.username.clone()),
        inputs: None,
        metadata,
    }
}

pub(crate) fn map_available_pipeline(project: &types::Project) -> AvailablePipeline {
    let parts: Vec<&str> = project.name_with_namespace.split('/').collect();
    let (organization, repository) = if parts.len() >= 2 {
        (Some(parts[0].to_string()), Some(parts[1..].join("/")))
    } else {
        (None, Some(project.name.clone()))
    };

    let id_without_spaces = project.name_with_namespace.replace(" ", "");

    AvailablePipeline {
        id: id_without_spaces,
        name: project.name.clone(),
        description: project.description.clone(),
        organization,
        repository,
    }
}

fn job_log_availability(job: &types::Job) -> Option<LogAvailability> {
    match job.status.as_str() {
        "running" | "canceling" => Some(LogAvailability::Partial),
        "created" | "pending" | "preparing" | "waiting_for_resource" | "scheduled" | "manual" => {
            Some(LogAvailability::Pending)
        }
        "skipped" => None,
        _ => job.started_at.map(|_| LogAvailability::Ready),
    }
}

fn stage_status(statuses: &[PipelineStatus]) -> PipelineStatus {
    [
        PipelineStatus::Failed,
        PipelineStatus::Running,
        PipelineStatus::Cancelled,
        PipelineStatus::Pending,
    ]
    .into_iter()
    .find(|status| statuses.contains(status))
    .unwrap_or_else(|| {
        if statuses
            .iter()
            .all(|status| *status == PipelineStatus::Skipped)
        {
            PipelineStatus::Skipped
        } else {
            PipelineStatus::Success
        }
    })
}

pub(crate) fn jobs_to_log_nodes(jobs: &[types::Job]) -> Vec<LogNode> {
    let mut attempts: HashMap<&str, (i64, &types::Job)> = HashMap::new();
    for job in jobs {
        attempts
            .entry(job.name.as_str())
            .and_modify(|(first, latest)| {
                *first = (*first).min(job.id);
                if job.id > latest.id {
                    *latest = job;
                }
            })
            .or_insert((job.id, job));
    }
    let mut latest: Vec<(i64, &types::Job)> = attempts.into_values().collect();
    latest.sort_by_key(|(first, _)| *first);

    let mut stages: Vec<(&str, Vec<&types::Job>)> = Vec::new();
    for (_, job) in latest {
        match stages.iter_mut().find(|(name, _)| *name == job.stage) {
            Some((_, members)) => members.push(job),
            None => stages.push((job.stage.as_str(), vec![job])),
        }
    }

    let mut nodes = Vec::with_capacity(jobs.len() + stages.len());
    for (stage, members) in stages {
        let stage_id = format!("stage:{stage}");
        let statuses: Vec<_> = members.iter().map(|job| map_status(&job.status)).collect();
        let status = stage_status(&statuses);
        let terminal = !matches!(status, PipelineStatus::Running | PipelineStatus::Pending);
        nodes.push(LogNode {
            id: stage_id.clone(),
            parent_id: None,
            kind: LogNodeKind::Stage,
            name: stage.to_string(),
            status,
            started_at: members.iter().filter_map(|job| job.started_at).min(),
            finished_at: members
                .iter()
                .filter_map(|job| job.finished_at)
                .max()
                .filter(|_| terminal),
            log: None,
            web_url: None,
        });
        nodes.extend(members.into_iter().map(|job| LogNode {
            id: job.id.to_string(),
            parent_id: Some(stage_id.clone()),
            kind: LogNodeKind::Job,
            name: job.name.clone(),
            status: map_status(&job.status),
            started_at: job.started_at,
            finished_at: job.finished_at,
            log: job_log_availability(job),
            web_url: job.web_url.clone(),
        }));
    }
    nodes
}

pub(crate) mod commits;
pub(crate) mod targets;

#[cfg(test)]
mod tests;
