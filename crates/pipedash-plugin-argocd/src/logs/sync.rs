use std::fmt::Write as _;

use pipedash_plugin_api::logs::tagged::{
    write_line,
    write_line_at,
};
use pipedash_plugin_api::utils::duration_seconds;
use pipedash_plugin_api::{
    redact_url,
    LineMarker,
    PipelineStatus,
};

use super::{
    non_empty,
    SyncRecord,
};
use crate::mapper;
use crate::types::{
    OperationInitiator,
    OperationState,
    ResourceResult,
    RevisionHistory,
};

pub(crate) fn write_sync(sync: &SyncRecord<'_>, out: &mut Vec<u8>) {
    match sync {
        SyncRecord::Operation(operation) => write_operation(operation, out),
        SyncRecord::History(entry) => write_history(entry, out),
    }
}

fn write_operation(operation: &OperationState, out: &mut Vec<u8>) {
    let open = LineMarker::SectionStart { open: true };
    write_line_at(out, open, Some(operation.started_at), "Sync operation");
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
        write_line(
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
            write_line(
                out,
                LineMarker::Plain,
                None,
                &format!("Options: {}", options.join(", ")),
            );
        }
    }
    if operation.retry_count > 0 {
        write_line(
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
        write_line_at(out, marker, operation.finished_at, message);
    }
    match duration_seconds(Some(operation.started_at), operation.finished_at) {
        Some(seconds) => write_line_at(
            out,
            LineMarker::Plain,
            operation.finished_at,
            &format!("{} after {seconds}s", operation.phase),
        ),
        None => write_line(out, LineMarker::Plain, None, &operation.phase),
    }

    let resources = result.map_or(&[][..], |result| &result.resources);
    if resources.is_empty() {
        let text = if operation.finished_at.is_some() {
            "Argo CD recorded no resource results for this operation."
        } else {
            "No resource results yet."
        };
        write_line(out, LineMarker::Plain, None, text);
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
        write_line(
            out,
            LineMarker::SectionStart { open },
            None,
            &format!("{phase} phase, {} {noun}", lines.len()),
        );
        for (marker, text) in lines {
            write_line(out, marker, None, &text);
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
    write_line_at(
        out,
        LineMarker::SectionStart { open: true },
        entry.deploy_started_at.or(Some(entry.deployed_at)),
        "Sync",
    );
    write_initiator(&entry.initiated_by, out);
    if let Some(revision) = revision_text(entry.revision.as_deref(), &entry.revisions) {
        write_line(
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
        write_line(out, LineMarker::Plain, None, &text);
    }
    let deployed = match duration_seconds(entry.deploy_started_at, Some(entry.deployed_at)) {
        Some(seconds) => format!("Deployed after {seconds}s"),
        None => "Deployed".to_string(),
    };
    write_line_at(out, LineMarker::Plain, Some(entry.deployed_at), &deployed);
    write_line(
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
    write_line(out, LineMarker::Plain, None, &text);
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
