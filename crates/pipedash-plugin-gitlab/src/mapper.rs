use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
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
    }
}

pub(crate) fn map_pipeline_run(
    pipeline: &types::Pipeline, project_id: i64, provider_id: i64,
) -> PipelineRun {
    let duration = pipeline.duration.or_else(|| {
        pipeline.started_at.and_then(|started| {
            pipeline
                .finished_at
                .map(|finished| (finished - started).num_seconds())
        })
    });

    PipelineRun {
        id: format!("gitlab__{}__{}_{}", provider_id, project_id, pipeline.id),
        pipeline_id: format!("gitlab__{}__{}", provider_id, project_id),
        run_number: pipeline.id,
        status: map_status(&pipeline.status),
        started_at: pipeline.started_at.unwrap_or(pipeline.created_at),
        concluded_at: pipeline.finished_at,
        duration_seconds: duration,
        logs_url: pipeline.web_url.clone(),
        commit_sha: pipeline.sha.clone(),
        commit_message: None,
        branch: pipeline.ref_name.clone(),
        actor: pipeline
            .user
            .as_ref()
            .map(|u| u.username.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        inputs: None,
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
