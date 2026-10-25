use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};

use crate::{
    config,
    types,
};

pub(crate) fn map_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    match (status, conclusion) {
        ("completed", Some("success")) => PipelineStatus::Success,
        ("completed", Some("cancelled")) => PipelineStatus::Cancelled,
        ("completed", Some("skipped")) => PipelineStatus::Skipped,
        ("completed", _) => PipelineStatus::Failed,
        ("in_progress", _) | ("queued", _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
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
    provider_id: i64, owner: &str, repo: &str, workflow: &types::Workflow,
    latest_run: Option<&types::Run>,
) -> Pipeline {
    Pipeline {
        id: config::build_pipeline_id(provider_id, owner, repo, &workflow.id),
        provider_id,
        provider_type: "gitea".to_string(),
        name: workflow.name.clone(),
        status: latest_run
            .map(|run| map_status(&run.status, run.conclusion.as_deref()))
            .unwrap_or(PipelineStatus::Pending),
        last_run: latest_run.and_then(|run| timestamp(run.started_at)),
        last_updated: Utc::now(),
        repository: format!("{owner}/{repo}"),
        branch: latest_run.map(|run| run.head_branch.clone()),
        workflow_file: Some(workflow.path.clone()),
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
        duration_seconds: started_at
            .zip(concluded_at)
            .map(|(started, concluded)| (concluded - started).num_seconds()),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_mapping_follows_github_style_status_and_conclusion() {
        let cases = [
            ("completed", Some("success"), PipelineStatus::Success),
            ("completed", Some("failure"), PipelineStatus::Failed),
            ("completed", Some("cancelled"), PipelineStatus::Cancelled),
            ("completed", Some("skipped"), PipelineStatus::Skipped),
            ("in_progress", None, PipelineStatus::Running),
            ("queued", None, PipelineStatus::Running),
            ("waiting", None, PipelineStatus::Pending),
            ("unknown", None, PipelineStatus::Pending),
        ];
        for (status, conclusion, expected) in cases {
            assert_eq!(map_status(status, conclusion), expected, "{status}");
        }
    }

    #[test]
    fn epoch_zero_timestamps_are_treated_as_unset() {
        let run: types::Run = serde_json::from_value(serde_json::json!({
            "id": 2, "run_number": 2, "status": "queued", "path": "ci.yaml@refs/heads/main",
            "html_url": "http://g/o/r/actions/runs/2",
            "started_at": "1970-01-01T00:00:00Z", "completed_at": "1970-01-01T00:00:00Z"
        }))
        .unwrap();
        let mapped = map_run(&run, "gitea__1__o/r/ci.yaml", "o");
        assert_eq!(mapped.concluded_at, None);
        assert_eq!(mapped.duration_seconds, None);
        assert_eq!(run.workflow_file(), "ci.yaml");
    }
}
