use std::fmt::Write as _;

use pipedash_plugin_api::{
    from_status,
    redact_url,
    LineMarker,
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
    PluginError,
    PluginResult,
};

use crate::log_dialect::{
    push_line,
    push_line_at,
};
use crate::mapper;
use crate::types::{
    Application,
    ApplicationTree,
    ContainerStatus,
    Event,
    HealthStatus,
    OperationInitiator,
    OperationState,
    Pod,
    PodLogMessage,
    ResourceResult,
    RevisionHistory,
};

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

pub(crate) fn write_pod_summary(
    pod: &Pod, namespace: &str, name: &str, events: Vec<Event>, out: &mut Vec<u8>,
) {
    push_line(
        out,
        LineMarker::SectionStart { open: true },
        None,
        &format!("Pod {namespace}/{name}"),
    );
    let mut phase = format!(
        "Phase {}",
        Some(pod.status.phase.as_str())
            .filter(|phase| !phase.is_empty())
            .unwrap_or("Unknown")
    );
    if let Some(node) = non_empty(&pod.spec.node_name) {
        let _ = write!(phase, " on node {node}");
    }
    push_line(out, LineMarker::Plain, None, &phase);
    let reason = [
        non_empty(&pod.status.reason),
        non_empty(&pod.status.message),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !reason.is_empty() {
        push_line(out, LineMarker::Warning, None, &reason.join(": "));
    }
    let statuses = pod
        .status
        .init_container_statuses
        .iter()
        .map(|status| (status, true))
        .chain(
            pod.status
                .container_statuses
                .iter()
                .map(|status| (status, false)),
        );
    for (status, init) in statuses {
        let (marker, text) = container_line(status, init);
        push_line(out, marker, None, &text);
    }

    let warned = events.iter().any(|event| event.event_type == "Warning");
    push_line(
        out,
        LineMarker::SectionStart { open: warned },
        None,
        "Events",
    );
    write_events(events, out);
}

fn container_line(status: &ContainerStatus, init: bool) -> (LineMarker, String) {
    let label = if init { "Init container" } else { "Container" };
    let mut text = format!("{label} {}: ", status.name);
    let mut detail = None;
    let failing = match (&status.state.waiting, &status.state.terminated) {
        (Some(waiting), _) => {
            let reason = non_empty(&waiting.reason);
            text.push_str("waiting");
            if let Some(reason) = reason {
                let _ = write!(text, " ({reason})");
            }
            detail = non_empty(&waiting.message);
            !matches!(reason, None | Some("ContainerCreating" | "PodInitializing"))
        }
        (None, Some(terminated)) => {
            let _ = write!(text, "exited with code {}", terminated.exit_code);
            if let Some(reason) = non_empty(&terminated.reason) {
                let _ = write!(text, " ({reason})");
            }
            detail = non_empty(&terminated.message);
            terminated.exit_code != 0
        }
        (None, None) => {
            text.push_str("running");
            false
        }
    };
    if status.restart_count > 0 {
        let _ = write!(text, ", {} restarts", status.restart_count);
    }
    if let Some(last) = &status.last_state.terminated {
        let _ = write!(text, ", last exit code {}", last.exit_code);
        if let Some(reason) = non_empty(&last.reason) {
            let _ = write!(text, " ({reason})");
        }
    }
    if let Some(detail) = detail {
        let _ = write!(text, ": {detail}");
    }
    let marker = if failing {
        LineMarker::Error
    } else if status.restart_count > 0 {
        LineMarker::Warning
    } else {
        LineMarker::Plain
    };
    (marker, text)
}

pub(crate) struct ContainerLog<'a> {
    pub(crate) name: &'a str,
    pub(crate) previous: bool,
    title: String,
    open: bool,
}

impl ContainerLog<'_> {
    pub(crate) fn write_header(&self, out: &mut Vec<u8>) {
        push_line(
            out,
            LineMarker::SectionStart { open: self.open },
            None,
            &self.title,
        );
    }
}

pub(crate) fn container_logs(pod: &Pod) -> Vec<ContainerLog<'_>> {
    let containers = pod
        .spec
        .init_containers
        .iter()
        .map(|container| (container, true))
        .chain(
            pod.spec
                .containers
                .iter()
                .map(|container| (container, false)),
        );
    let mut logs = Vec::new();
    for (container, init) in containers {
        let statuses = if init {
            &pod.status.init_container_statuses
        } else {
            &pod.status.container_statuses
        };
        let status = statuses.iter().find(|status| status.name == container.name);
        let label = if init { "Init container" } else { "Container" };
        let title = format!("{label} {}", container.name);
        if status.is_some_and(|status| {
            status.restart_count > 0 && status.last_state.terminated.is_some()
        }) {
            logs.push(ContainerLog {
                name: &container.name,
                previous: true,
                title: format!("{title}, previous attempt"),
                open: true,
            });
        }
        let completed = init
            && status
                .and_then(|status| status.state.terminated.as_ref())
                .is_some_and(|terminated| terminated.exit_code == 0);
        logs.push(ContainerLog {
            name: &container.name,
            previous: false,
            title,
            open: !completed,
        });
    }
    logs
}

pub(crate) fn write_no_output(out: &mut Vec<u8>) {
    push_line(out, LineMarker::Plain, None, "No log output.");
}

#[derive(Debug, Default)]
pub(crate) struct PodLogDecoder {
    pending: Vec<u8>,
    pub(crate) lines: usize,
    pub(crate) done: bool,
}

impl PodLogDecoder {
    pub(crate) fn feed(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> PluginResult<()> {
        self.pending.extend_from_slice(chunk);
        let Some(newline) = self.pending.iter().rposition(|b| *b == b'\n') else {
            return Ok(());
        };
        let rest = self.pending.split_off(newline + 1);
        let complete = std::mem::replace(&mut self.pending, rest);
        complete
            .split(|b| *b == b'\n')
            .try_for_each(|message| self.message(message, out))
    }

    pub(crate) fn finish(&mut self, out: &mut Vec<u8>) -> PluginResult<()> {
        let rest = std::mem::take(&mut self.pending);
        self.message(&rest, out)
    }

    fn message(&mut self, raw: &[u8], out: &mut Vec<u8>) -> PluginResult<()> {
        let raw = raw.trim_ascii();
        if raw.is_empty() || self.done {
            return Ok(());
        }
        let message: PodLogMessage = serde_json::from_slice(raw).map_err(|e| {
            PluginError::ApiError(format!("Failed to parse ArgoCD log stream: {e}"))
        })?;
        if let Some(error) = message.error {
            return Err(from_status(error.http_code, error.message, None));
        }
        let Some(entry) = message.result else {
            return Ok(());
        };
        if entry.last {
            self.done = true;
            if entry.content.is_empty() {
                return Ok(());
            }
        }
        let stamp = entry
            .time_stamp_str
            .as_deref()
            .filter(|stamp| !stamp.starts_with("0001-"));
        let marker = if stamp.is_some() {
            LineMarker::Plain
        } else {
            LineMarker::Warning
        };
        push_line(out, marker, stamp, &entry.content);
        self.lines += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pipedash_plugin_api::LogDialect;

    use super::*;
    use crate::log_dialect::ArgocdLogDialect;

    const STREAM: &str = concat!(
        r#"{"result":{"content":"listening on :8080","timeStampStr":"2026-10-27T10:05:01.123456789Z","podName":"web-1","last":false}}"#,
        "\n",
        r#"{"result":{"content":"container \"app\" in pod \"web-1\" is waiting to start: ContainerCreating","timeStampStr":"0001-01-01T00:00:00Z","podName":"","last":false}}"#,
        "\n",
        r#"{"result":{"content":"","timeStampStr":"2026-10-27T10:05:02Z","podName":"","last":true}}"#,
        "\n",
        r#"{"result":{"content":"after the end","timeStampStr":"2026-10-27T10:05:03Z","podName":"web-1","last":false}}"#,
        "\n",
    );

    fn decode(chunks: &[&[u8]]) -> PluginResult<(Vec<u8>, PodLogDecoder)> {
        let mut decoder = PodLogDecoder::default();
        let mut out = Vec::new();
        for chunk in chunks {
            decoder.feed(chunk, &mut out)?;
        }
        decoder.finish(&mut out)?;
        Ok((out, decoder))
    }

    #[test]
    fn stream_entries_become_timestamped_lines_until_the_last_entry() {
        let (out, decoder) = decode(&[STREAM.as_bytes()]).unwrap();

        let lines: Vec<_> = out
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| ArgocdLogDialect.parse(line))
            .collect();
        assert_eq!(decoder.lines, 2);
        assert!(decoder.done);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].marker, LineMarker::Plain);
        assert_eq!(lines[0].body, b"listening on :8080");
        assert_eq!(
            lines[0].timestamp,
            Some(
                chrono::DateTime::parse_from_rfc3339("2026-10-27T10:05:01.123456789Z")
                    .unwrap()
                    .to_utc()
            )
        );
        assert_eq!(lines[1].marker, LineMarker::Warning);
        assert_eq!(lines[1].timestamp, None);
    }

    #[test]
    fn entries_split_across_chunks_decode_the_same() {
        let whole = decode(&[STREAM.as_bytes()]).unwrap().0;
        for cut in [1, 17, 130, STREAM.len() - 1] {
            let (head, tail) = STREAM.as_bytes().split_at(cut);
            assert_eq!(decode(&[head, tail]).unwrap().0, whole, "cut at {cut}");
        }
    }

    #[test]
    fn stream_errors_keep_their_http_status() {
        let error = decode(&[
            br#"{"error":{"grpc_code":7,"http_code":403,"message":"permission denied: logs, get, default/web","http_status":"Forbidden"}}"#,
        ])
        .unwrap_err();
        assert!(
            matches!(&error, PluginError::PermissionDenied(message) if message.contains("logs, get")),
            "{error:?}"
        );

        let error = decode(&[b"<html>proxy error</html>\n"]).unwrap_err();
        assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
    }

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

    #[test]
    fn pod_node_ids_accept_only_kubernetes_names() {
        assert_eq!(
            pod_of_node("pod:web:web-7f9c.abc"),
            Some(("web", "web-7f9c.abc"))
        );
        for id in [
            "",
            "sync",
            "pod:",
            "pod:web",
            "pod:web:",
            "pod:Web:a",
            "pod:web:a:b",
            "pod:web:a/b",
            "pod/web/a",
            "pod:web:a?b=1",
            "pod:..:a",
            "pod:web:-a",
        ] {
            assert_eq!(pod_of_node(id), None, "{id}");
        }
    }
}
