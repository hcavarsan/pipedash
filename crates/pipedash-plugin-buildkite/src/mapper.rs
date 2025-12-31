use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    BuildAgent,
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
}
