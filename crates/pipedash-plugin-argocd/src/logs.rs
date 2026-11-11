use std::fmt::Write as _;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::logs::tagged::{
    self,
    LogEvent,
};
use pipedash_plugin_api::{
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
};

use crate::mapper;
use crate::types::logs::{
    ApplicationTree,
    Event,
};
use crate::types::{
    Application,
    HealthStatus,
    OperationState,
    RevisionHistory,
};

pub(crate) mod decoder;
pub(crate) mod pods;
pub(crate) mod sync;

pub(crate) const SYNC_NODE: &str = "sync";
pub(crate) const EVENTS_NODE: &str = "events";
pub(crate) const POD_LOG_TAIL_LINES: &str = "1000";
const PODS_NODE: &str = "pods";
const POD_NODE_PREFIX: &str = "pod:";
const NO_EVENTS: &str = "No events. Kubernetes keeps events for one hour by default.";

pub(crate) enum SyncRecord<'a> {
    Operation(&'a OperationState),
    History(&'a RevisionHistory),
}

pub(crate) fn find_sync(app: &Application, run_number: i64) -> Option<SyncRecord<'_>> {
    let operation = app.status.operation_state.as_ref();
    if let Some(operation) = operation.filter(|op| op.started_at.timestamp() == run_number) {
        return Some(SyncRecord::Operation(operation));
    }
    let entry = app
        .status
        .history
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|entry| entry.deployed_at.timestamp() == run_number)?;
    Some(match operation {
        Some(operation) if entry.deploy_started_at == Some(operation.started_at) => {
            SyncRecord::Operation(operation)
        }
        _ => SyncRecord::History(entry),
    })
}

pub(crate) fn log_nodes(
    sync: &SyncRecord<'_>, tree: &ApplicationTree, web_url: &str,
) -> Vec<LogNode> {
    let node = |id: String, parent: Option<&str>, kind, name: &str, status, log| LogNode {
        id,
        parent_id: parent.map(str::to_string),
        kind,
        name: name.to_string(),
        status,
        started_at: None,
        finished_at: None,
        log,
        web_url: (!web_url.is_empty()).then(|| web_url.to_string()),
    };

    let (status, started_at, finished_at) = match sync {
        SyncRecord::Operation(operation) => (
            mapper::operation_status(&operation.phase),
            Some(operation.started_at),
            operation.finished_at,
        ),
        SyncRecord::History(entry) => (
            PipelineStatus::Success,
            entry.deploy_started_at,
            Some(entry.deployed_at),
        ),
    };
    let sync_log = match status {
        PipelineStatus::Running | PipelineStatus::Pending => LogAvailability::Partial,
        _ => LogAvailability::Ready,
    };

    let mut nodes = vec![
        LogNode {
            started_at,
            finished_at,
            ..node(
                SYNC_NODE.to_string(),
                None,
                LogNodeKind::Job,
                "Sync result",
                status,
                Some(sync_log),
            )
        },
        node(
            EVENTS_NODE.to_string(),
            None,
            LogNodeKind::Job,
            "Application events",
            PipelineStatus::Success,
            Some(LogAvailability::Partial),
        ),
    ];

    let mut pods: Vec<_> = tree
        .nodes
        .iter()
        .filter(|resource| resource.kind == "Pod" && resource.group.is_empty())
        .collect();
    if pods.is_empty() {
        return nodes;
    }
    pods.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    let statuses: Vec<_> = pods
        .iter()
        .map(|pod| health_status(pod.health.as_ref()))
        .collect();
    let stage_status = [
        PipelineStatus::Failed,
        PipelineStatus::Running,
        PipelineStatus::Pending,
    ]
    .into_iter()
    .find(|status| statuses.contains(status))
    .unwrap_or(PipelineStatus::Success);

    nodes.push(node(
        PODS_NODE.to_string(),
        None,
        LogNodeKind::Stage,
        "Pods",
        stage_status,
        None,
    ));
    nodes.extend(pods.iter().zip(statuses).map(|(pod, status)| {
        node(
            format!("{POD_NODE_PREFIX}{}:{}", pod.namespace, pod.name),
            Some(PODS_NODE),
            LogNodeKind::Job,
            &pod.name,
            status,
            Some(LogAvailability::Partial),
        )
    }));
    nodes
}

fn health_status(health: Option<&HealthStatus>) -> PipelineStatus {
    match health.map(|health| health.status.as_str()) {
        Some("Healthy") => PipelineStatus::Success,
        Some("Degraded" | "Missing") => PipelineStatus::Failed,
        Some("Progressing") => PipelineStatus::Running,
        Some("Suspended") => PipelineStatus::Cancelled,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn pod_of_node(node_id: &str) -> Option<(&str, &str)> {
    let (namespace, name) = node_id.strip_prefix(POD_NODE_PREFIX)?.split_once(':')?;
    let valid = |part: &str| {
        part.bytes()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    };
    (valid(namespace) && valid(name)).then_some((namespace, name))
}

fn non_empty(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|text| !text.is_empty())
}

impl LogEvent for Event {
    fn is_warning(&self) -> bool {
        self.event_type == "Warning"
    }

    fn event_time(&self) -> Option<DateTime<Utc>> {
        self.time()
    }

    fn event_text(&self) -> String {
        let mut text = format!("{} {}: {}", self.event_type, self.reason, self.message);
        if self.count > 1 {
            let _ = write!(text, " (x{})", self.count);
        }
        text
    }
}

pub(crate) fn write_events(events: Vec<Event>, out: &mut Vec<u8>) {
    tagged::write_events(out, &events, NO_EVENTS);
}

#[cfg(test)]
mod tests;
