use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::utils::{
    duration_seconds,
    parse_timestamp,
    repository_name,
};
use pipedash_plugin_api::{
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

use crate::types;

pub(crate) fn map_build_state(state: &str) -> PipelineStatus {
    match state {
        "passed" => PipelineStatus::Success,
        "failed" => PipelineStatus::Failed,
        "running" | "started" => PipelineStatus::Running,
        "scheduled" | "creating" | "waiting" => PipelineStatus::Pending,
        "canceled" | "canceling" => PipelineStatus::Cancelled,
        "skipped" | "not_run" => PipelineStatus::Skipped,
        "blocked" => PipelineStatus::Pending,
        _ => PipelineStatus::Pending,
    }
}

fn map_job_state(state: &str) -> PipelineStatus {
    match state {
        "passed" => PipelineStatus::Success,
        "failed" | "timed_out" => PipelineStatus::Failed,
        "running" | "canceling" | "timing_out" => PipelineStatus::Running,
        "canceled" | "expired" => PipelineStatus::Cancelled,
        "skipped" | "broken" | "waiting_failed" | "blocked_failed" | "unblocked_failed" => {
            PipelineStatus::Skipped
        }
        _ => PipelineStatus::Pending,
    }
}

fn job_name(job: &types::Job) -> String {
    let base = job
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            job.command
                .as_deref()
                .and_then(|command| command.lines().find(|line| !line.trim().is_empty()))
        })
        .or(job.step_key.as_deref())
        .unwrap_or("Job")
        .trim();
    match (job.parallel_group_index, job.parallel_group_total) {
        (Some(index), Some(total)) if total > 1 => format!("{base} ({}/{total})", index + 1),
        _ => base.to_string(),
    }
}

pub(crate) fn jobs_to_log_nodes(jobs: &[types::Job]) -> Vec<LogNode> {
    jobs.iter()
        .filter(|job| job.kind.as_deref() == Some("script") && job.retried != Some(true))
        .map(|job| {
            let status = map_job_state(job.state.as_deref().unwrap_or_default());
            let started_at = job.started_at.as_deref().and_then(parse_timestamp);
            let log = match status {
                PipelineStatus::Running => Some(LogAvailability::Partial),
                PipelineStatus::Pending => Some(LogAvailability::Pending),
                _ => started_at.map(|_| LogAvailability::Ready),
            };
            LogNode {
                id: job.id.clone(),
                parent_id: None,
                kind: LogNodeKind::Job,
                name: job_name(job),
                status,
                started_at,
                finished_at: job.finished_at.as_deref().and_then(parse_timestamp),
                log,
                web_url: job.web_url.clone(),
            }
        })
        .collect()
}

pub(crate) fn map_pipeline(
    org: &str, slug: &str, pipeline: types::Pipeline, latest_build: Option<&types::Build>,
) -> Pipeline {
    let metadata = HashMap::from([("organization_slug".to_string(), serde_json::json!(org))]);

    Pipeline {
        correlation: Correlation::single_source(RepoId::parse(&pipeline.repository), None),
        id: format!("{org}__{slug}"),
        name: pipeline.name,
        status: latest_build.map_or(PipelineStatus::Pending, |build| {
            map_build_state(&build.state)
        }),
        last_run: latest_build.and_then(|build| parse_timestamp(&build.created_at)),
        last_updated: Utc::now(),
        repository: repository_name(&pipeline.repository),
        branch: latest_build
            .map(|build| build.branch.clone())
            .filter(|branch| !branch.is_empty()),
        workflow_file: None,
        resource_state: None,
        metadata,
    }
}

pub(crate) fn build_to_pipeline_run(build: types::Build, pipeline_id: &str) -> PipelineRun {
    let status = map_build_state(&build.state);

    let started = build.started_at.as_deref().and_then(parse_timestamp);
    let concluded_at = build.finished_at.as_deref().and_then(parse_timestamp);
    let started_at = started
        .or_else(|| parse_timestamp(&build.created_at))
        .unwrap_or_else(Utc::now);
    let duration_seconds = duration_seconds(started, concluded_at);

    let branch_value = if build.branch.is_empty() {
        "unknown".to_string()
    } else {
        build.branch.clone()
    };

    let mut inputs_map = serde_json::Map::new();
    inputs_map.insert(
        "branch".to_string(),
        serde_json::Value::String(branch_value.clone()),
    );
    inputs_map.insert(
        "commit".to_string(),
        serde_json::Value::String(build.commit.clone()),
    );

    tracing::debug!(
        build_number = build.number,
        branch = %branch_value,
        commit = %build.commit,
        "Processing Buildkite build"
    );

    PipelineRun {
        correlation: Correlation::single_source(
            build
                .pipeline
                .as_ref()
                .and_then(|pipeline| pipeline.repository.as_deref())
                .and_then(RepoId::parse),
            CommitSha::parse(&build.commit),
        ),
        evidence: RunEvidence::default(),
        id: format!("{pipeline_id}__{}", build.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: build.number,
        status,
        started_at,
        concluded_at,
        duration_seconds,
        logs_url: build.web_url.clone(),
        commit_sha: if build.commit.is_empty() {
            None
        } else {
            Some(build.commit.clone())
        },
        commit_message: build.message.clone(),
        branch: Some(branch_value),
        actor: build.author.as_ref().map(|a| a.name.clone()),
        inputs: Some(serde_json::Value::Object(inputs_map)),
        metadata: HashMap::new(),
    }
}

#[cfg(test)]
mod tests;
