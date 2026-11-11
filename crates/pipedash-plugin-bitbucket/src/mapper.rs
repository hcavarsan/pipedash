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

pub(crate) fn map_status(state: &types::PipelineState) -> PipelineStatus {
    match state.name.as_str() {
        "PENDING" | "READY" => PipelineStatus::Pending,
        "PARSING" => PipelineStatus::Running,
        "IN_PROGRESS" => {
            let is_paused = state
                .stage
                .as_ref()
                .map(|s| s.name == "PAUSED")
                .unwrap_or(false);
            if is_paused {
                PipelineStatus::Pending
            } else {
                PipelineStatus::Running
            }
        }
        "PAUSED" | "HALTED" => PipelineStatus::Pending,
        "COMPLETED" => match state.result.as_ref().map(|r| r.name.as_str()) {
            Some("SUCCESSFUL") => PipelineStatus::Success,
            Some("FAILED") | Some("ERROR") => PipelineStatus::Failed,
            Some("STOPPED") | Some("EXPIRED") => PipelineStatus::Cancelled,
            Some("NOT_RUN") => PipelineStatus::Skipped,
            _ => PipelineStatus::Failed, // Unknown result -> Failed (safer than Success)
        },
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn steps_to_log_nodes(steps: &[types::PipelineStep]) -> Vec<LogNode> {
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let status = map_status(&step.state);
            let log = match status {
                PipelineStatus::Running => Some(LogAvailability::Partial),
                PipelineStatus::Pending => Some(LogAvailability::Pending),
                _ => step.started_on.map(|_| LogAvailability::Ready),
            };
            LogNode {
                id: step.uuid.clone(),
                parent_id: None,
                kind: LogNodeKind::Job,
                name: step
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map_or_else(|| format!("Step {}", index + 1), str::to_string),
                status,
                started_at: step.started_on,
                finished_at: step.completed_on,
                log,
                web_url: None,
            }
        })
        .collect()
}

pub(crate) fn map_pipeline(
    repo: &types::Repository, latest_pipeline: Option<&types::Pipeline>,
) -> Pipeline {
    let status = latest_pipeline
        .map(|p| map_status(&p.state))
        .unwrap_or(PipelineStatus::Pending);

    let last_run = latest_pipeline.map(|p| p.created_on);
    let branch = latest_pipeline.and_then(|p| p.target.ref_name.clone());

    let mut metadata = HashMap::new();
    metadata.insert(
        "workspace".to_string(),
        serde_json::json!(repo.workspace.slug),
    );
    metadata.insert("repo_slug".to_string(), serde_json::json!(repo.slug));

    Pipeline {
        id: format!("{}__{}", repo.workspace.slug, repo.slug),
        name: repo.name.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository: repo.full_name.clone(),
        branch,
        workflow_file: Some("bitbucket-pipelines.yml".to_string()),
        resource_state: None,
        metadata,
    }
}

pub(crate) fn map_pipeline_run(
    pipeline: &types::Pipeline, workspace: &str, repo_slug: &str,
) -> PipelineRun {
    let mut metadata = HashMap::new();
    metadata.insert("workspace".to_string(), serde_json::json!(workspace));

    if let Some(ref selector) = pipeline.target.selector {
        metadata.insert(
            "selector_type".to_string(),
            serde_json::json!(selector.selector_type),
        );
        if let Some(ref pattern) = selector.pattern {
            metadata.insert("selector_pattern".to_string(), serde_json::json!(pattern));
        }
    }

    let logs_url = pipeline
        .links
        .html
        .as_ref()
        .map(|l| l.href.clone())
        .unwrap_or_else(|| {
            format!(
                "https://bitbucket.org/{}/{}/pipelines/results/{}",
                workspace, repo_slug, pipeline.build_number
            )
        });

    PipelineRun {
        id: format!("{workspace}__{repo_slug}__{}", pipeline.build_number),
        pipeline_id: format!("{workspace}__{repo_slug}"),
        run_number: pipeline.build_number,
        status: map_status(&pipeline.state),
        started_at: pipeline.created_on,
        concluded_at: pipeline.completed_on,
        duration_seconds: pipeline.duration_in_seconds,
        logs_url,
        commit_sha: pipeline.target.commit.as_ref().map(|c| c.hash.clone()),
        commit_message: pipeline
            .target
            .commit
            .as_ref()
            .and_then(|c| c.message.clone()),
        branch: pipeline.target.ref_name.clone(),
        actor: pipeline.creator.as_ref().map(|u| u.display_name.clone()),
        inputs: None,
        metadata,
    }
}

pub(crate) fn map_available_pipeline(repo: &types::Repository) -> AvailablePipeline {
    AvailablePipeline {
        id: repo.full_name.clone(),
        name: repo.name.clone(),
        description: repo.description.clone(),
        organization: Some(repo.workspace.slug.clone()),
        repository: Some(repo.slug.clone()),
    }
}

#[cfg(test)]
mod tests;
