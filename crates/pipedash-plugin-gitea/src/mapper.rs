use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
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
            nodes.push(LogNode {
                id: format!("{job_id}:{}", step.number),
                parent_id: Some(job_id.clone()),
                kind: LogNodeKind::Step,
                name: step.name.clone(),
                status: map_status(&step.status, step.conclusion.as_deref()),
                started_at: timestamp(step.started_at),
                finished_at: timestamp(step.completed_at),
                log: None,
                web_url: None,
            });
        }
    }

    nodes
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

    fn jobs(value: serde_json::Value) -> Vec<types::Job> {
        serde_json::from_value::<types::JobList>(value)
            .unwrap()
            .jobs
    }

    #[test]
    fn every_job_status_word_maps_to_a_status_and_a_log_availability() {
        let started = "2026-10-28T10:00:00Z";
        let unset = "1970-01-01T00:00:00Z";
        let cases = [
            (
                "completed",
                "success",
                started,
                PipelineStatus::Success,
                Some(LogAvailability::Ready),
            ),
            (
                "completed",
                "failure",
                started,
                PipelineStatus::Failed,
                Some(LogAvailability::Ready),
            ),
            (
                "completed",
                "cancelled",
                started,
                PipelineStatus::Cancelled,
                Some(LogAvailability::Ready),
            ),
            (
                "completed",
                "cancelled",
                unset,
                PipelineStatus::Cancelled,
                None,
            ),
            ("completed", "skipped", unset, PipelineStatus::Skipped, None),
            (
                "in_progress",
                "",
                started,
                PipelineStatus::Running,
                Some(LogAvailability::Partial),
            ),
            (
                "queued",
                "",
                unset,
                PipelineStatus::Running,
                Some(LogAvailability::Pending),
            ),
            (
                "waiting",
                "",
                unset,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending),
            ),
            (
                "",
                "",
                unset,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending),
            ),
        ];
        let list = cases
            .iter()
            .enumerate()
            .map(|(i, (status, conclusion, started_at, ..))| {
                serde_json::json!({
                    "id": i, "name": format!("job{i}"), "status": status,
                    "conclusion": Some(conclusion).filter(|c| !c.is_empty()),
                    "started_at": started_at, "completed_at": unset,
                })
            })
            .collect::<Vec<_>>();

        let nodes = jobs_to_log_nodes(&jobs(serde_json::json!({ "jobs": list })));

        assert_eq!(nodes.len(), cases.len());
        for (node, (status, conclusion, _, expected_status, expected_log)) in
            nodes.iter().zip(cases)
        {
            let word = format!("{status}/{conclusion}");
            assert_eq!(node.status, expected_status, "{word}");
            assert_eq!(node.log, expected_log, "{word}");
        }
    }

    #[test]
    fn steps_become_children_of_their_job_without_a_log_of_their_own() {
        let nodes = jobs_to_log_nodes(&jobs(serde_json::json!({"jobs": [
            {
                "id": 101, "name": "build", "status": "completed", "conclusion": "failure",
                "html_url": "http://g/o/r/actions/runs/7/jobs/0",
                "started_at": "2026-10-28T10:00:00Z", "completed_at": "2026-10-28T10:01:00Z",
                "steps": [
                    {"name": "Set up job", "number": 0, "status": "completed",
                     "conclusion": "success", "started_at": "2026-10-28T10:00:01Z",
                     "completed_at": "2026-10-28T10:00:03Z"},
                    {"name": "Run tests", "number": 1, "status": "completed",
                     "conclusion": "failure", "started_at": "2026-10-28T10:00:03Z",
                     "completed_at": "2026-10-28T10:00:58Z"}
                ]
            },
            {
                "id": 102, "name": "deploy", "status": "queued", "steps": null,
                "started_at": "1970-01-01T00:00:00Z", "completed_at": "1970-01-01T00:00:00Z"
            }
        ]})));

        let shape: Vec<_> = nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.parent_id.as_deref(),
                    node.kind,
                    node.log,
                )
            })
            .collect();
        assert_eq!(
            shape,
            [
                ("101", None, LogNodeKind::Job, Some(LogAvailability::Ready)),
                ("101:0", Some("101"), LogNodeKind::Step, None),
                ("101:1", Some("101"), LogNodeKind::Step, None),
                (
                    "102",
                    None,
                    LogNodeKind::Job,
                    Some(LogAvailability::Pending)
                ),
            ]
        );
        assert_eq!(
            nodes[0].web_url.as_deref(),
            Some("http://g/o/r/actions/runs/7/jobs/0")
        );
        assert_eq!(nodes[1].name, "Set up job");
        assert_eq!(nodes[1].status, PipelineStatus::Success);
        assert_eq!(nodes[2].status, PipelineStatus::Failed);
        assert_eq!(
            nodes[2].started_at,
            Some("2026-10-28T10:00:03Z".parse().unwrap())
        );
        assert_eq!(
            nodes[2].finished_at,
            Some("2026-10-28T10:00:58Z".parse().unwrap())
        );
        assert_eq!(nodes[3].started_at, None);
        assert_eq!(nodes[3].finished_at, None);
        assert_eq!(nodes[3].web_url, None);
    }
}
