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

use crate::types;

pub(crate) fn map_status(gitlab_status: &str) -> PipelineStatus {
    match gitlab_status {
        "success" => PipelineStatus::Success,
        "failed" => PipelineStatus::Failed,
        "running" | "pending" => PipelineStatus::Running,
        "canceled" => PipelineStatus::Cancelled,
        "skipped" => PipelineStatus::Skipped,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn map_pipeline(
    project: &types::Project, pipeline: Option<&types::Pipeline>, provider_id: i64,
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
        id: format!("gitlab__{}__{}", provider_id, project.id),
        provider_id,
        provider_type: "gitlab".to_string(),
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
    pipeline: &types::Pipeline, project_id: i64, provider_id: i64, namespace: Option<&str>,
) -> PipelineRun {
    let duration = pipeline.duration.or_else(|| {
        pipeline.started_at.and_then(|started| {
            pipeline
                .finished_at
                .map(|finished| (finished - started).num_seconds())
        })
    });

    let mut metadata = HashMap::new();
    if let Some(ref source) = pipeline.source {
        metadata.insert("source".to_string(), serde_json::json!(source));
    }
    if let Some(ns) = namespace {
        metadata.insert("namespace".to_string(), serde_json::json!(ns));
    }

    PipelineRun {
        id: format!("gitlab__{}__{}_{}", provider_id, project_id, pipeline.id),
        pipeline_id: format!("gitlab__{}__{}", provider_id, project_id),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: i64, name: &str, stage: &str, status: &str, started: bool) -> types::Job {
        types::Job {
            id,
            name: name.to_string(),
            stage: stage.to_string(),
            status: status.to_string(),
            started_at: started.then(|| "2026-10-27T10:00:00Z".parse().unwrap()),
            finished_at: None,
            web_url: None,
        }
    }

    #[test]
    fn every_job_status_word_maps_to_a_status_and_a_log() {
        let cases = [
            (
                "created",
                false,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending),
            ),
            (
                "pending",
                false,
                PipelineStatus::Running,
                Some(LogAvailability::Pending),
            ),
            (
                "preparing",
                false,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending),
            ),
            (
                "waiting_for_resource",
                false,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending),
            ),
            (
                "running",
                true,
                PipelineStatus::Running,
                Some(LogAvailability::Partial),
            ),
            (
                "success",
                true,
                PipelineStatus::Success,
                Some(LogAvailability::Ready),
            ),
            (
                "failed",
                true,
                PipelineStatus::Failed,
                Some(LogAvailability::Ready),
            ),
            ("failed", false, PipelineStatus::Failed, None),
            (
                "canceled",
                true,
                PipelineStatus::Cancelled,
                Some(LogAvailability::Ready),
            ),
            ("canceled", false, PipelineStatus::Cancelled, None),
            (
                "canceling",
                true,
                PipelineStatus::Pending,
                Some(LogAvailability::Partial),
            ),
            ("skipped", false, PipelineStatus::Skipped, None),
            (
                "manual",
                false,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending),
            ),
            (
                "scheduled",
                false,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending),
            ),
        ];
        for (word, started, status, log) in cases {
            let nodes = jobs_to_log_nodes(&[job(1, "j", "s", word, started)]);
            assert_eq!(nodes[1].status, status, "{word}");
            assert_eq!(nodes[1].log, log, "{word} started={started}");
        }
    }

    #[test]
    fn stage_status_summarizes_its_jobs() {
        let cases = [
            (vec!["success", "failed", "running"], PipelineStatus::Failed),
            (vec!["success", "running"], PipelineStatus::Running),
            (vec!["success", "canceled"], PipelineStatus::Cancelled),
            (vec!["success", "manual"], PipelineStatus::Pending),
            (vec!["skipped", "skipped"], PipelineStatus::Skipped),
            (vec!["success", "skipped"], PipelineStatus::Success),
        ];
        for (words, expected) in cases {
            let jobs: Vec<_> = words
                .iter()
                .enumerate()
                .map(|(i, word)| job(i as i64 + 1, &format!("j{i}"), "s", word, true))
                .collect();
            assert_eq!(jobs_to_log_nodes(&jobs)[0].status, expected, "{words:?}");
        }
    }

    #[test]
    fn a_retried_stage_keeps_its_place_and_its_latest_job() {
        let jobs = [
            job(30, "test", "test", "success", true),
            job(3, "ship", "deploy", "skipped", false),
            job(2, "test", "test", "failed", true),
            job(1, "build", "build", "success", true),
        ];

        let nodes = jobs_to_log_nodes(&jobs);

        let order: Vec<_> = nodes.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(
            order,
            ["stage:build", "1", "stage:test", "30", "stage:deploy", "3"]
        );
    }
}
