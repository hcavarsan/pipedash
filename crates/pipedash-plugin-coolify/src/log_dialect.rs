use std::collections::HashSet;

use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
    PluginError,
    PluginResult,
};

use crate::types::LogEntry;

const TAGS: [(u8, LineMarker); 4] = [
    (b'.', LineMarker::Plain),
    (b'$', LineMarker::Command),
    (b'!', LineMarker::Error),
    (b'~', LineMarker::Debug),
];

pub(crate) struct CoolifyLogDialect;

impl LogDialect for CoolifyLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let plain = ParsedLine {
            timestamp: None,
            marker: LineMarker::Plain,
            body: line,
        };
        let Some((tag, rest)) = line.split_first() else {
            return plain;
        };
        let Some(text) = rest.strip_prefix(b" ") else {
            return plain;
        };
        match TAGS.iter().find(|(known, _)| known == tag) {
            Some((_, LineMarker::Command)) => ParsedLine {
                marker: LineMarker::Command,
                ..plain
            },
            Some((_, marker)) => ParsedLine {
                timestamp: None,
                marker: *marker,
                body: text,
            },
            None => plain,
        }
    }

    fn infers_errors(&self) -> bool {
        false
    }
}

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
    let tag = TAGS
        .iter()
        .find(|(_, known)| *known == marker)
        .map_or(b'.', |(tag, _)| *tag);
    let line = line.strip_suffix('\r').unwrap_or(line);
    out.extend_from_slice(&[tag, b' ']);
    for (index, segment) in line.split('\r').enumerate() {
        if index > 0 {
            out.extend_from_slice(&[b'\r', tag, b' ']);
        }
        out.extend_from_slice(segment.as_bytes());
    }
    out.push(b'\n');
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entries(value: serde_json::Value) -> Vec<LogEntry> {
        parse_entries(&value.to_string()).unwrap()
    }

    #[test]
    fn every_tag_maps_to_its_marker_and_commands_keep_their_prompt() {
        let cases: [(&[u8], LineMarker, &[u8]); 4] = [
            (b". cloning", LineMarker::Plain, b"cloning"),
            (
                b"$ docker build .",
                LineMarker::Command,
                b"$ docker build .",
            ),
            (b"! build failed", LineMarker::Error, b"build failed"),
            (b"~ docker inspect", LineMarker::Debug, b"docker inspect"),
        ];
        for (line, marker, body) in cases {
            let parsed = CoolifyLogDialect.parse(line);
            assert_eq!((parsed.marker, parsed.body), (marker, body), "{line:?}");
        }
        let untagged = CoolifyLogDialect.parse(b"~no space");
        assert_eq!(untagged.marker, LineMarker::Plain);
    }

    #[test]
    fn entries_render_in_order_with_commands_once_per_batch() {
        let rendered = render(&entries(json!([
            {"command": "docker build .", "output": "step 2", "type": "stdout", "hidden": false, "batch": 1, "order": 3},
            {"command": "docker build .", "output": "step 1", "type": "stdout", "hidden": false, "batch": 1, "order": 2},
            {"command": null, "output": "Starting deployment", "type": "stdout", "hidden": false, "batch": 0, "order": null},
            {"command": "docker build .", "output": "boom\nretry", "type": "stderr", "hidden": false, "batch": 2, "order": 4},
            {"command": "docker inspect x", "output": "{}", "type": "stdout", "hidden": true, "batch": 3, "order": 5}
        ])));

        assert_eq!(
            String::from_utf8(rendered).unwrap(),
            ". Starting deployment\n$ docker build .\n. step 1\n. step 2\n$ docker build .\n! boom\n! retry\n$ docker inspect x\n~ {}\n"
        );
    }

    #[test]
    fn blank_logs_have_no_entries_and_broken_logs_are_an_api_error() {
        assert!(parse_entries("").unwrap().is_empty());
        assert!(parse_entries("  \n").unwrap().is_empty());
        assert!(matches!(parse_entries("{"), Err(PluginError::ApiError(_))));
    }
}
