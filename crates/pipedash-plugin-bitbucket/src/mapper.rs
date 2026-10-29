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
    repo: &types::Repository, latest_pipeline: Option<&types::Pipeline>, provider_id: i64,
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
        id: format!(
            "bitbucket__{}__{}__{}",
            provider_id, repo.workspace.slug, repo.slug
        ),
        provider_id,
        provider_type: "bitbucket".to_string(),
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
    pipeline: &types::Pipeline, workspace: &str, repo_slug: &str, provider_id: i64,
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
        id: format!(
            "bitbucket__{}__{}__{}__{}",
            provider_id, workspace, repo_slug, pipeline.build_number
        ),
        pipeline_id: format!("bitbucket__{}__{}__{}", provider_id, workspace, repo_slug),
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
mod tests {
    use super::*;

    fn state(json: serde_json::Value) -> types::PipelineState {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn every_pipeline_and_step_state_maps_to_a_status() {
        let completed =
            |result: &str| serde_json::json!({"name": "COMPLETED", "result": {"name": result}});
        let cases = [
            (
                serde_json::json!({"name": "PENDING"}),
                PipelineStatus::Pending,
            ),
            (
                serde_json::json!({"name": "READY"}),
                PipelineStatus::Pending,
            ),
            (
                serde_json::json!({"name": "PARSING"}),
                PipelineStatus::Running,
            ),
            (
                serde_json::json!({"name": "IN_PROGRESS"}),
                PipelineStatus::Running,
            ),
            (
                serde_json::json!({"name": "IN_PROGRESS", "stage": {"name": "PAUSED"}}),
                PipelineStatus::Pending,
            ),
            (
                serde_json::json!({"name": "PAUSED"}),
                PipelineStatus::Pending,
            ),
            (
                serde_json::json!({"name": "HALTED"}),
                PipelineStatus::Pending,
            ),
            (completed("SUCCESSFUL"), PipelineStatus::Success),
            (completed("FAILED"), PipelineStatus::Failed),
            (completed("ERROR"), PipelineStatus::Failed),
            (completed("STOPPED"), PipelineStatus::Cancelled),
            (completed("EXPIRED"), PipelineStatus::Cancelled),
            (completed("NOT_RUN"), PipelineStatus::Skipped),
            (
                serde_json::json!({"name": "COMPLETED"}),
                PipelineStatus::Failed,
            ),
            (
                serde_json::json!({"name": "SOMETHING_NEW"}),
                PipelineStatus::Pending,
            ),
        ];
        for (json, status) in cases {
            assert_eq!(map_status(&state(json.clone())), status, "{json}");
        }
    }

    #[test]
    fn steps_become_job_nodes_with_the_availability_of_their_log() {
        let steps: Vec<types::PipelineStep> = serde_json::from_str(
            r#"[
                {"uuid":"{a}","name":"Build","state":{"name":"COMPLETED","result":{"name":"SUCCESSFUL"}},
                 "started_on":"2026-10-28T10:00:00.000Z","completed_on":"2026-10-28T10:01:15.000Z"},
                {"uuid":"{b}","name":" Test ","state":{"name":"COMPLETED","result":{"name":"FAILED"}},
                 "started_on":"2026-10-28T10:01:16.000Z","completed_on":"2026-10-28T10:02:00.000Z"},
                {"uuid":"{c}","name":"Deploy","state":{"name":"IN_PROGRESS"},
                 "started_on":"2026-10-28T10:02:01.000Z"},
                {"uuid":"{d}","name":"Approve","state":{"name":"PENDING","stage":{"name":"PAUSED"}}},
                {"uuid":"{e}","name":null,"state":{"name":"READY"}},
                {"uuid":"{f}","name":"Smoke","state":{"name":"COMPLETED","result":{"name":"NOT_RUN"}}},
                {"uuid":"{g}","name":"Release","state":{"name":"COMPLETED","result":{"name":"STOPPED"}},
                 "started_on":"2026-10-28T10:03:00.000Z","completed_on":"2026-10-28T10:03:10.000Z"},
                {"uuid":"{h}","name":"Cleanup","state":{"name":"COMPLETED","result":{"name":"STOPPED"}}}
            ]"#,
        )
        .unwrap();

        let nodes = steps_to_log_nodes(&steps);

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| (node.id.as_str(), node.name.as_str(), node.status, node.log))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "{a}",
                    "Build",
                    PipelineStatus::Success,
                    Some(LogAvailability::Ready)
                ),
                (
                    "{b}",
                    "Test",
                    PipelineStatus::Failed,
                    Some(LogAvailability::Ready)
                ),
                (
                    "{c}",
                    "Deploy",
                    PipelineStatus::Running,
                    Some(LogAvailability::Partial)
                ),
                (
                    "{d}",
                    "Approve",
                    PipelineStatus::Pending,
                    Some(LogAvailability::Pending)
                ),
                (
                    "{e}",
                    "Step 5",
                    PipelineStatus::Pending,
                    Some(LogAvailability::Pending)
                ),
                ("{f}", "Smoke", PipelineStatus::Skipped, None),
                (
                    "{g}",
                    "Release",
                    PipelineStatus::Cancelled,
                    Some(LogAvailability::Ready)
                ),
                ("{h}", "Cleanup", PipelineStatus::Cancelled, None),
            ]
        );
        assert!(nodes
            .iter()
            .all(|node| node.kind == LogNodeKind::Job && node.parent_id.is_none()));
        assert_eq!(
            nodes[0].finished_at.unwrap() - nodes[0].started_at.unwrap(),
            chrono::Duration::seconds(75)
        );
        assert!(nodes[2].started_at.is_some() && nodes[2].finished_at.is_none());
        assert!(nodes[3].started_at.is_none());
    }
}
