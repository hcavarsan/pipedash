use octocrab::models::workflows::Job;
use pipedash_plugin_api::{
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
    ResourceState,
};
use serde::Serialize;

pub(crate) fn map_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    match (status, conclusion) {
        ("completed", Some("success" | "neutral")) => PipelineStatus::Success,
        ("completed", Some("cancelled" | "stale")) => PipelineStatus::Cancelled,
        ("completed", Some("skipped")) => PipelineStatus::Skipped,
        ("completed", _) => PipelineStatus::Failed,
        ("in_progress", _) | ("queued", _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn disabled_state(workflow_state: &str) -> Option<ResourceState> {
    let reason = match workflow_state {
        "disabled_inactivity" => {
            "GitHub disabled this workflow after 60 days without repository activity"
        }
        "disabled_manually" => "Someone disabled this workflow on GitHub",
        "disabled_fork" => "GitHub disables workflows in a fork until you enable them",
        _ => return None,
    };
    Some(ResourceState::new("Disabled", Some(reason)))
}

fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) fn jobs_to_log_nodes(jobs: &[Job]) -> Vec<LogNode> {
    let mut nodes = Vec::with_capacity(jobs.iter().map(|job| job.steps.len() + 1).sum());

    for job in jobs {
        let job_id = job.id.to_string();
        let status_wire = wire(&job.status);
        let not_started = matches!(
            status_wire.as_str(),
            "queued" | "waiting" | "pending" | "requested"
        );

        nodes.push(LogNode {
            id: job_id.clone(),
            parent_id: None,
            kind: LogNodeKind::Job,
            name: job.name.clone(),
            status: map_status(&status_wire, job.conclusion.as_ref().map(wire).as_deref()),
            started_at: (!not_started).then_some(job.started_at),
            finished_at: job.completed_at,
            log: Some(if status_wire == "completed" {
                LogAvailability::Ready
            } else {
                LogAvailability::Pending
            }),
            web_url: Some(job.html_url.to_string()),
        });

        for step in &job.steps {
            nodes.push(LogNode {
                id: format!("{job_id}:{}", step.number),
                parent_id: Some(job_id.clone()),
                kind: LogNodeKind::Step,
                name: step.name.clone(),
                status: map_status(
                    &wire(&step.status),
                    step.conclusion.as_ref().map(wire).as_deref(),
                ),
                started_at: step.started_at,
                finished_at: step.completed_at,
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
    fn every_documented_conclusion_maps_to_a_finished_status() {
        let cases = [
            ("success", PipelineStatus::Success),
            ("neutral", PipelineStatus::Success),
            ("failure", PipelineStatus::Failed),
            ("timed_out", PipelineStatus::Failed),
            ("startup_failure", PipelineStatus::Failed),
            ("action_required", PipelineStatus::Failed),
            ("cancelled", PipelineStatus::Cancelled),
            ("stale", PipelineStatus::Cancelled),
            ("skipped", PipelineStatus::Skipped),
        ];
        for (conclusion, expected) in cases {
            assert_eq!(
                map_status("completed", Some(conclusion)),
                expected,
                "conclusion {conclusion}"
            );
        }
    }

    #[test]
    fn unfinished_statuses_are_running_or_pending() {
        assert_eq!(map_status("in_progress", None), PipelineStatus::Running);
        assert_eq!(map_status("queued", None), PipelineStatus::Running);
        for status in ["requested", "waiting", "pending"] {
            assert_eq!(
                map_status(status, None),
                PipelineStatus::Pending,
                "status {status}"
            );
        }
    }

    #[test]
    fn every_documented_workflow_state_maps_to_its_disabled_reason() {
        let cases = [
            ("active", None),
            ("deleted", None),
            (
                "disabled_inactivity",
                Some("GitHub disabled this workflow after 60 days without repository activity"),
            ),
            (
                "disabled_manually",
                Some("Someone disabled this workflow on GitHub"),
            ),
            (
                "disabled_fork",
                Some("GitHub disables workflows in a fork until you enable them"),
            ),
        ];
        for (workflow_state, reason) in cases {
            assert_eq!(
                disabled_state(workflow_state),
                reason.map(|reason| ResourceState::new("Disabled", Some(reason))),
                "state {workflow_state}"
            );
        }
    }
}
