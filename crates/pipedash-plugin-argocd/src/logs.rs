use std::fmt::Write as _;

use pipedash_plugin_api::{
    redact_url,
    LineMarker,
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
};

use crate::log_dialect::{
    push_line,
    push_line_at,
};
use crate::mapper;
use crate::types::{
    Application,
    Event,
    OperationInitiator,
    OperationState,
    ResourceResult,
    RevisionHistory,
};

pub(crate) const SYNC_NODE: &str = "sync";
pub(crate) const EVENTS_NODE: &str = "events";
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

pub(crate) fn log_nodes(sync: &SyncRecord<'_>, web_url: &str) -> Vec<LogNode> {
    let node = |id: &str, name: &str, status, log| LogNode {
        id: id.to_string(),
        parent_id: None,
        kind: LogNodeKind::Job,
        name: name.to_string(),
        status,
        started_at: None,
        finished_at: None,
        log: Some(log),
        web_url: Some(web_url.to_string()),
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

    vec![
        LogNode {
            started_at,
            finished_at,
            ..node(SYNC_NODE, "Sync result", status, sync_log)
        },
        node(
            EVENTS_NODE,
            "Application events",
            PipelineStatus::Success,
            LogAvailability::Partial,
        ),
    ]
}

fn non_empty(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|text| !text.is_empty())
}

pub(crate) fn write_sync(sync: &SyncRecord<'_>, out: &mut Vec<u8>) {
    match sync {
        SyncRecord::Operation(operation) => write_operation(operation, out),
        SyncRecord::History(entry) => write_history(entry, out),
    }
}

fn write_operation(operation: &OperationState, out: &mut Vec<u8>) {
    let open = LineMarker::SectionStart { open: true };
    push_line_at(out, open, Some(operation.started_at), "Sync operation");
    write_initiator(&operation.operation.initiated_by, out);

    let result = operation.sync_result.as_ref();
    let revision = result
        .and_then(|result| revision_text(result.revision.as_deref(), &result.revisions))
        .or_else(|| {
            operation
                .operation
                .sync
                .as_ref()
                .and_then(|sync| non_empty(&sync.revision).map(str::to_string))
        });
    if let Some(revision) = revision {
        push_line(
            out,
            LineMarker::Plain,
            None,
            &format!("Revision {revision}"),
        );
    }
    if let Some(sync) = &operation.operation.sync {
        let options: Vec<_> = [(sync.prune, "prune"), (sync.dry_run, "dry run")]
            .into_iter()
            .filter_map(|(on, name)| on.then_some(name))
            .collect();
        if !options.is_empty() {
            push_line(
                out,
                LineMarker::Plain,
                None,
                &format!("Options: {}", options.join(", ")),
            );
        }
    }
    if operation.retry_count > 0 {
        push_line(
            out,
            LineMarker::Plain,
            None,
            &format!("Retried {} times", operation.retry_count),
        );
    }

    let status = mapper::operation_status(&operation.phase);
    if let Some(message) = non_empty(&operation.message) {
        let marker = if status == PipelineStatus::Failed {
            LineMarker::Error
        } else {
            LineMarker::Plain
        };
        push_line_at(out, marker, operation.finished_at, message);
    }
    match operation.finished_at {
        Some(finished) => push_line_at(
            out,
            LineMarker::Plain,
            Some(finished),
            &format!(
                "{} after {}s",
                operation.phase,
                (finished - operation.started_at).num_seconds()
            ),
        ),
        None => push_line(out, LineMarker::Plain, None, &operation.phase),
    }

    let resources = result.map_or(&[][..], |result| &result.resources);
    if resources.is_empty() {
        let text = if operation.finished_at.is_some() {
            "Argo CD recorded no resource results for this operation."
        } else {
            "No resource results yet."
        };
        push_line(out, LineMarker::Plain, None, text);
        return;
    }
    let mut phases: Vec<(&str, Vec<&ResourceResult>)> = Vec::new();
    for resource in resources {
        let phase = non_empty(&resource.sync_phase).unwrap_or("Sync");
        match phases.iter_mut().find(|(known, _)| *known == phase) {
            Some((_, members)) => members.push(resource),
            None => phases.push((phase, vec![resource])),
        }
    }
    for (phase, members) in phases {
        let lines: Vec<_> = members.into_iter().map(resource_line).collect();
        let open = lines.iter().any(|(marker, _)| *marker != LineMarker::Plain);
        let noun = if lines.len() == 1 {
            "resource"
        } else {
            "resources"
        };
        push_line(
            out,
            LineMarker::SectionStart { open },
            None,
            &format!("{phase} phase, {} {noun}", lines.len()),
        );
        for (marker, text) in lines {
            push_line(out, marker, None, &text);
        }
    }
}

fn resource_line(resource: &ResourceResult) -> (LineMarker, String) {
    let status = non_empty(&resource.status);
    let hook_phase = non_empty(&resource.hook_phase);
    let state = status.or(hook_phase).unwrap_or("Pending");
    let mut text = format!("{state:<12} ");
    if !resource.group.is_empty() {
        let _ = write!(text, "{}/", resource.group);
    }
    let _ = write!(text, "{} ", resource.kind);
    if !resource.namespace.is_empty() {
        let _ = write!(text, "{}/", resource.namespace);
    }
    text.push_str(&resource.name);
    if let Some(hook) = non_empty(&resource.hook_type) {
        let _ = write!(text, " ({hook} hook)");
    }
    if let Some(message) = non_empty(&resource.message) {
        let _ = write!(text, ": {message}");
    }
    let marker = if status == Some("SyncFailed") || matches!(hook_phase, Some("Failed" | "Error")) {
        LineMarker::Error
    } else if status == Some("PruneSkipped") {
        LineMarker::Warning
    } else {
        LineMarker::Plain
    };
    (marker, text)
}

fn write_history(entry: &RevisionHistory, out: &mut Vec<u8>) {
    push_line_at(
        out,
        LineMarker::SectionStart { open: true },
        entry.deploy_started_at.or(Some(entry.deployed_at)),
        "Sync",
    );
    write_initiator(&entry.initiated_by, out);
    if let Some(revision) = revision_text(entry.revision.as_deref(), &entry.revisions) {
        push_line(
            out,
            LineMarker::Plain,
            None,
            &format!("Revision {revision}"),
        );
    }
    if let Some(source) = entry
        .source
        .as_ref()
        .filter(|source| !source.repo_url.is_empty())
    {
        let repo = reqwest::Url::parse(&source.repo_url)
            .map_or_else(|_| source.repo_url.clone(), |url| redact_url(&url));
        let mut text = format!("Source {repo}");
        if let Some(path) = non_empty(&source.path).or(non_empty(&source.chart)) {
            let _ = write!(text, " {path}");
        }
        push_line(out, LineMarker::Plain, None, &text);
    }
    let deployed = match entry.deploy_started_at {
        Some(started) => format!(
            "Deployed after {}s",
            (entry.deployed_at - started).num_seconds()
        ),
        None => "Deployed".to_string(),
    };
    push_line_at(out, LineMarker::Plain, Some(entry.deployed_at), &deployed);
    push_line(
        out,
        LineMarker::Plain,
        None,
        "Argo CD keeps per-resource results only for the most recent operation of an app.",
    );
}

fn write_initiator(initiator: &OperationInitiator, out: &mut Vec<u8>) {
    let text = match non_empty(&initiator.username) {
        Some(user) => format!("Started by {user}"),
        None if initiator.automated => "Started by the automated sync policy".to_string(),
        None => return,
    };
    push_line(out, LineMarker::Plain, None, &text);
}

fn revision_text(revision: Option<&str>, revisions: &[String]) -> Option<String> {
    if revisions.is_empty() {
        revision
            .filter(|revision| !revision.is_empty())
            .map(str::to_string)
    } else {
        Some(revisions.join(", "))
    }
}

pub(crate) fn write_events(mut events: Vec<Event>, out: &mut Vec<u8>) {
    if events.is_empty() {
        push_line(out, LineMarker::Plain, None, NO_EVENTS);
        return;
    }
    events.sort_by_key(Event::time);
    for event in &events {
        let marker = if event.event_type == "Warning" {
            LineMarker::Warning
        } else {
            LineMarker::Plain
        };
        let mut text = format!("{} {}: {}", event.event_type, event.reason, event.message);
        if event.count > 1 {
            let _ = write!(text, " (x{})", event.count);
        }
        push_line_at(out, marker, event.time(), &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(history: &str) -> Application {
        serde_json::from_str(&format!(
            r#"{{"metadata":{{"name":"web"}},"spec":{{"source":{{"repoURL":"https://github.com/acme/web"}},"destination":{{}},"project":"default"}},
                "status":{{"sync":{{"status":"Synced"}},"health":{{"status":"Healthy"}},"history":{history},
                "operationState":{{"phase":"Succeeded","startedAt":"2026-10-27T10:00:00Z","finishedAt":"2026-10-27T10:00:40Z"}}}}}}"#
        ))
        .unwrap()
    }

    fn ts(stamp: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(stamp)
            .unwrap()
            .timestamp()
    }

    #[test]
    fn history_entry_of_the_latest_operation_shows_the_operation() {
        let app = app(
            r#"[{"revision":"abc","deployedAt":"2026-10-26T09:00:30Z","deployStartedAt":"2026-10-26T09:00:00Z","id":1},
                {"revision":"def","deployedAt":"2026-10-27T10:00:41Z","deployStartedAt":"2026-10-27T10:00:00Z","id":2}]"#,
        );

        let cases = [
            ("2026-10-27T10:00:00Z", Some("operation")),
            ("2026-10-27T10:00:41Z", Some("operation")),
            ("2026-10-26T09:00:30Z", Some("history 1")),
            ("2026-10-26T09:00:00Z", None),
        ];
        for (stamp, expected) in cases {
            let found = find_sync(&app, ts(stamp)).map(|sync| match sync {
                SyncRecord::Operation(_) => "operation".to_string(),
                SyncRecord::History(entry) => format!("history {}", entry.id),
            });
            assert_eq!(found.as_deref(), expected, "{stamp}");
        }
    }
}
