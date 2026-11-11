use std::fmt::Write as _;

use pipedash_plugin_api::logs::tagged::write_line;
use pipedash_plugin_api::LineMarker;

use super::{
    non_empty,
    write_events,
};
use crate::types::logs::{
    ContainerStatus,
    Event,
    Pod,
};

pub(crate) fn write_pod_summary(
    pod: &Pod, namespace: &str, name: &str, events: Vec<Event>, out: &mut Vec<u8>,
) {
    write_line(
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
    write_line(out, LineMarker::Plain, None, &phase);
    let reason = [
        non_empty(&pod.status.reason),
        non_empty(&pod.status.message),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !reason.is_empty() {
        write_line(out, LineMarker::Warning, None, &reason.join(": "));
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
        write_line(out, marker, None, &text);
    }

    let warned = events.iter().any(|event| event.event_type == "Warning");
    write_line(
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
        write_line(
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
    write_line(out, LineMarker::Plain, None, "No log output.");
}
