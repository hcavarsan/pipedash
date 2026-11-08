use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const MARKERS: [(&[u8], LineMarker); 8] = [
    (b"##[group]", LineMarker::GroupStart),
    (b"##[endgroup]", LineMarker::GroupEnd),
    (b"##[error]", LineMarker::Error),
    (b"##[warning]", LineMarker::Warning),
    (b"##[notice]", LineMarker::Notice),
    (b"##[debug]", LineMarker::Debug),
    (b"##[command]", LineMarker::Command),
    (b"[command]", LineMarker::Command),
];

pub struct ActionsLogDialect;

impl LogDialect for ActionsLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let line = line.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(line);
        let (timestamp, body) = split_timestamp(line);
        for (prefix, marker) in MARKERS {
            if let Some(rest) = body.strip_prefix(prefix) {
                return ParsedLine {
                    timestamp,
                    marker,
                    body: rest,
                };
            }
        }
        ParsedLine {
            timestamp,
            marker: LineMarker::Plain,
            body,
        }
    }

    fn is_step_start(&self, line: &ParsedLine<'_>) -> bool {
        match line.marker {
            LineMarker::GroupStart => line.body.starts_with(b"Run "),
            LineMarker::Plain => line.body == b"Post job cleanup.",
            _ => false,
        }
    }

    fn infers_errors(&self) -> bool {
        false
    }
}

fn split_timestamp(line: &[u8]) -> (Option<DateTime<Utc>>, &[u8]) {
    if line.len() <= 20 || line[4] != b'-' || line[10] != b'T' {
        return (None, line);
    }
    let head = &line[..line.len().min(40)];
    let Some(space) = head.iter().position(|b| *b == b' ') else {
        return (None, line);
    };
    std::str::from_utf8(&line[..space])
        .ok()
        .and_then(pipedash_plugin_api::utils::parse_timestamp)
        .map_or((None, line), |ts| (Some(ts), &line[space + 1..]))
}

#[cfg(test)]
mod tests;
