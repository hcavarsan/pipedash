use std::fmt::Write as _;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::logs::tagged::{
    self,
    write_line,
    write_line_at,
    LogEvent,
};
use pipedash_plugin_api::{
    LineMarker,
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineRun,
    PipelineStatus,
    PluginError,
    PluginResult,
};
use serde_json::Value;

use crate::{
    mapper,
    types::{
        Event,
        FluxResource,
        Kind,
    },
};

pub(crate) const RECONCILIATION_NODE: &str = "reconciliation";
pub(crate) const EVENTS_NODE: &str = "object-events";
const NO_EVENTS: &str = "No events. Kubernetes keeps events for one hour by default.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Node {
    Reconciliation,
    Events,
}

impl Node {
    pub fn parse(id: &str) -> PluginResult<Self> {
        match id {
            RECONCILIATION_NODE => Ok(Self::Reconciliation),
            EVENTS_NODE => Ok(Self::Events),
            _ => Err(PluginError::InvalidConfig(format!(
                "Invalid FluxCD log node: {id}"
            ))),
        }
    }
}

pub(crate) fn log_nodes(run: &PipelineRun) -> Vec<LogNode> {
    let node = |id: &str, name: &str, status, log| LogNode {
        id: id.to_string(),
        parent_id: None,
        kind: LogNodeKind::Job,
        name: name.to_string(),
        status,
        started_at: None,
        finished_at: None,
        log: Some(log),
        web_url: None,
    };
    vec![
        LogNode {
            started_at: Some(run.started_at),
            finished_at: run.concluded_at,
            ..node(
                RECONCILIATION_NODE,
                "Reconciliation",
                run.status,
                LogAvailability::Partial,
            )
        },
        node(
            EVENTS_NODE,
            "Events of the object",
            PipelineStatus::Success,
            LogAvailability::Partial,
        ),
    ]
}

fn text<'a>(run: &'a PipelineRun, key: &str) -> Option<&'a str> {
    run.metadata
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn field(run: &PipelineRun, key: &str) -> Option<String> {
    match run.metadata.get(key)? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub(crate) fn write_reconciliation(resource: &FluxResource, run: &PipelineRun, out: &mut Vec<u8>) {
    let target = mapper::target_of(resource);
    let current = mapper::describes_current_state(run);
    let marker = match run.status {
        PipelineStatus::Failed => LineMarker::Error,
        PipelineStatus::Skipped => LineMarker::Warning,
        _ => LineMarker::Plain,
    };

    write_line_at(
        out,
        LineMarker::SectionStart { open: true },
        Some(run.started_at),
        &format!("{} of {target}", text(run, "run_label").unwrap_or("Run")),
    );
    let mut status = text(run, "status_label").unwrap_or("Unknown").to_string();
    let has_own_reason = !current && !run.metadata.contains_key("release_status");
    if let Some(reason) = text(run, "reason").filter(|_| has_own_reason) {
        let _ = write!(status, ": {reason}");
    }
    write_line_at(out, marker, run.concluded_at, &status);
    if let Some(message) = text(run, "message").filter(|_| current) {
        let reason = text(run, "condition_reason").or(text(run, "reason"));
        let line = format!("{}: {message}", reason.unwrap_or("Status"));
        write_line(out, marker, None, &line);
    }
    if let Some(seconds) = run.duration_seconds {
        write_line(out, LineMarker::Plain, None, &format!("Took {seconds}s"));
    }

    let revision = match resource.kind {
        Kind::Kustomization => "Revision",
        Kind::HelmRelease => "Chart version",
    };
    let fields = [
        (revision, "revision"),
        ("Origin revision", "origin_revision"),
        ("Applied revision", "applied_revision"),
        ("Source", "source_ref"),
        ("Chart", "chart"),
        ("Release version", "release_version"),
        ("App version", "app_version"),
        ("Action", "action"),
        ("Digest", "digest"),
        ("Config digest", "config_digest"),
        ("First reconciled", "first_reconciled"),
        ("First deployed", "first_deployed"),
        ("Reconciliations", "total_reconciliations"),
        ("Interval", "interval"),
    ];
    for (label, key) in fields {
        if let Some(value) = field(run, key) {
            write_line(out, LineMarker::Plain, None, &format!("{label}: {value}"));
        }
    }

    if !current || resource.status.conditions.is_empty() {
        return;
    }
    write_line(
        out,
        LineMarker::SectionStart { open: true },
        None,
        "Conditions",
    );
    for condition in &resource.status.conditions {
        let failing = matches!(
            (condition.type_.as_str(), condition.status.as_str()),
            ("Ready", "False") | ("Stalled", "True")
        );
        let mut line = format!(
            "{}={} {}",
            condition.type_, condition.status, condition.reason
        );
        if !condition.message.is_empty() {
            let _ = write!(line, ": {}", condition.message);
        }
        let marker = if failing {
            LineMarker::Error
        } else {
            LineMarker::Plain
        };
        write_line_at(out, marker, condition.last_transition_time, &line);
    }
}

impl LogEvent for Event {
    fn is_warning(&self) -> bool {
        self.type_ == "Warning"
    }

    fn event_time(&self) -> Option<DateTime<Utc>> {
        self.time()
    }

    fn event_text(&self) -> String {
        let mut text = format!("{} {}", self.type_, self.reason);
        if let Some(component) = self.component() {
            let _ = write!(text, " from {component}");
        }
        let _ = write!(text, ": {}", self.message);
        if self.occurrences() > 1 {
            let _ = write!(text, " (x{})", self.occurrences());
        }
        text
    }
}

pub(crate) fn write_events(events: Vec<Event>, out: &mut Vec<u8>) {
    tagged::write_events(out, &events, NO_EVENTS);
}
