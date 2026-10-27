use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    BuildAgent,
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
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

fn parse_time(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|at| at.with_timezone(&Utc))
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
            let started_at = parse_time(job.started_at.as_deref());
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
                finished_at: parse_time(job.finished_at.as_deref()),
                log,
                web_url: job.web_url.clone(),
            }
        })
        .collect()
}

pub(crate) fn map_agent(agent: types::Agent) -> BuildAgent {
    let status = if agent.connected {
        if agent.job.is_some() {
            "busy".to_string()
        } else {
            "idle".to_string()
        }
    } else {
        "disconnected".to_string()
    };

    let mut metadata = HashMap::new();
    metadata.insert("ip_address".to_string(), agent.ip_address);
    metadata.insert("version".to_string(), agent.version);

    BuildAgent {
        id: agent.id,
        name: agent.name,
        hostname: agent.hostname,
        status,
        job_id: agent.job.map(|j| j.id),
        last_seen: Utc::now(),
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_build_state() {
        assert_eq!(map_build_state("passed"), PipelineStatus::Success);
        assert_eq!(map_build_state("failed"), PipelineStatus::Failed);
        assert_eq!(map_build_state("running"), PipelineStatus::Running);
        assert_eq!(map_build_state("canceled"), PipelineStatus::Cancelled);
        assert_eq!(map_build_state("skipped"), PipelineStatus::Skipped);
        assert_eq!(map_build_state("unknown"), PipelineStatus::Pending);
    }

    #[test]
    fn every_job_state_maps_to_a_status() {
        let cases = [
            ("pending", PipelineStatus::Pending),
            ("waiting", PipelineStatus::Pending),
            ("blocked", PipelineStatus::Pending),
            ("unblocked", PipelineStatus::Pending),
            ("limiting", PipelineStatus::Pending),
            ("limited", PipelineStatus::Pending),
            ("scheduled", PipelineStatus::Pending),
            ("assigned", PipelineStatus::Pending),
            ("accepted", PipelineStatus::Pending),
            ("running", PipelineStatus::Running),
            ("canceling", PipelineStatus::Running),
            ("timing_out", PipelineStatus::Running),
            ("passed", PipelineStatus::Success),
            ("failed", PipelineStatus::Failed),
            ("timed_out", PipelineStatus::Failed),
            ("canceled", PipelineStatus::Cancelled),
            ("expired", PipelineStatus::Cancelled),
            ("skipped", PipelineStatus::Skipped),
            ("broken", PipelineStatus::Skipped),
            ("waiting_failed", PipelineStatus::Skipped),
            ("blocked_failed", PipelineStatus::Skipped),
            ("unblocked_failed", PipelineStatus::Skipped),
        ];
        for (state, status) in cases {
            assert_eq!(map_job_state(state), status, "{state}");
        }
    }

    #[test]
    fn script_jobs_of_the_latest_attempt_become_log_nodes() {
        let build: types::BuildJobs = serde_json::from_str(
            r#"{"number":2,"state":"failed","jobs":[
                {"id":"old","type":"script","name":"RSpec","state":"failed","retried":true,
                 "started_at":"2026-10-27T10:00:00.000Z","finished_at":"2026-10-27T10:01:00.000Z"},
                {"id":"new","type":"script","name":"RSpec","state":"passed","retried":false,
                 "web_url":"https://buildkite.com/acme/app/builds/2#new",
                 "started_at":"2026-10-27T10:02:00.000Z","finished_at":"2026-10-27T10:03:30.000Z"},
                {"id":"wait","type":"waiter","state":null},
                {"id":"gate","type":"manual","label":"Deploy?","state":"blocked"},
                {"id":"shard","type":"script","name":":rspec: Tests","state":"running",
                 "started_at":"2026-10-27T10:04:00.000Z","parallel_group_index":1,"parallel_group_total":4},
                {"id":"queued","type":"script","name":null,"command":"\nmake lint\nmake test","state":"scheduled"},
                {"id":"never","type":"script","step_key":"deploy","state":"broken"},
                {"id":"stopped","type":"script","name":"Lint","state":"canceled",
                 "started_at":"2026-10-27T10:05:00.000Z","finished_at":"2026-10-27T10:05:10.000Z"}
            ]}"#,
        )
        .unwrap();

        let nodes = jobs_to_log_nodes(&build.jobs);

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| (node.id.as_str(), node.name.as_str(), node.status, node.log))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "new",
                    "RSpec",
                    PipelineStatus::Success,
                    Some(LogAvailability::Ready)
                ),
                (
                    "shard",
                    ":rspec: Tests (2/4)",
                    PipelineStatus::Running,
                    Some(LogAvailability::Partial)
                ),
                (
                    "queued",
                    "make lint",
                    PipelineStatus::Pending,
                    Some(LogAvailability::Pending)
                ),
                ("never", "deploy", PipelineStatus::Skipped, None),
                (
                    "stopped",
                    "Lint",
                    PipelineStatus::Cancelled,
                    Some(LogAvailability::Ready)
                ),
            ]
        );
        assert!(nodes
            .iter()
            .all(|node| node.kind == LogNodeKind::Job && node.parent_id.is_none()));
        let new = &nodes[0];
        assert_eq!(
            new.web_url.as_deref(),
            Some("https://buildkite.com/acme/app/builds/2#new")
        );
        assert_eq!(
            new.finished_at.unwrap() - new.started_at.unwrap(),
            chrono::Duration::seconds(90)
        );
    }
}
