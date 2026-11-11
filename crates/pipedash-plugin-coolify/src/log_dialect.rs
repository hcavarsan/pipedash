use std::collections::HashSet;

use pipedash_plugin_api::logs::tagged::{
    write_spaced_line,
    SpacedTagDialect,
};
use pipedash_plugin_api::{
    LineMarker,
    PluginError,
    PluginResult,
};

use crate::types::LogEntry;

const TAGS: &[(u8, LineMarker)] = &[
    (b'.', LineMarker::Plain),
    (b'$', LineMarker::Command),
    (b'!', LineMarker::Error),
    (b'~', LineMarker::Debug),
];

pub(crate) const DIALECT: SpacedTagDialect = SpacedTagDialect { tags: TAGS };

pub(crate) fn parse_entries(logs: &str) -> PluginResult<Vec<LogEntry>> {
    if logs.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<LogEntry> = serde_json::from_str(logs).map_err(|e| {
        PluginError::ApiError(format!("Failed to parse Coolify deployment logs: {e}"))
    })?;
    entries.sort_by_key(|entry| entry.order);
    Ok(entries)
}

pub(crate) fn render(entries: &[LogEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut printed = HashSet::new();
    for entry in entries {
        if let Some(command) = entry.command.as_deref().filter(|c| !c.is_empty()) {
            if printed.insert((command, entry.batch)) {
                for line in command.lines() {
                    push_line(&mut out, LineMarker::Command, line);
                }
            }
        }
        let marker = if entry.hidden {
            LineMarker::Debug
        } else if entry.stream.as_deref() == Some("stderr") {
            LineMarker::Error
        } else {
            LineMarker::Plain
        };
        for line in entry.output.as_deref().unwrap_or_default().lines() {
            push_line(&mut out, marker, line);
        }
    }
    out
}

fn push_line(out: &mut Vec<u8>, marker: LineMarker, line: &str) {
    write_spaced_line(out, TAGS, marker, line);
}

#[cfg(test)]
mod tests;
