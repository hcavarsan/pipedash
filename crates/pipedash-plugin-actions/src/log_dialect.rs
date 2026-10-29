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
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map_or((None, line), |ts| {
            (Some(ts.with_timezone(&Utc)), &line[space + 1..])
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> ParsedLine<'_> {
        ActionsLogDialect.parse(line)
    }

    #[test]
    fn bom_timestamp_and_group_are_stripped() {
        let parsed = parse(b"\xEF\xBB\xBF2024-05-01T10:00:05.1234567Z ##[group]Run make");
        assert_eq!(parsed.marker, LineMarker::GroupStart);
        assert_eq!(parsed.body, b"Run make");
        assert_eq!(
            parsed.timestamp.unwrap().timestamp_millis(),
            DateTime::parse_from_rfc3339("2024-05-01T10:00:05.123Z")
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn bare_command_prefix_is_a_command() {
        let parsed = parse(b"2024-05-01T10:00:05.0000000Z [command]/usr/bin/git fetch");
        assert_eq!(parsed.marker, LineMarker::Command);
        assert_eq!(parsed.body, b"/usr/bin/git fetch");
    }

    #[test]
    fn line_without_timestamp_keeps_its_body() {
        let parsed = parse(b"plain output line here");
        assert_eq!(parsed.timestamp, None);
        assert_eq!(parsed.marker, LineMarker::Plain);
        assert_eq!(parsed.body, b"plain output line here");
    }

    #[test]
    fn malformed_timestamp_leaves_the_line_unchanged() {
        let line = b"2024-05-01T99:99:99.0000000Z hello";
        let parsed = parse(line);
        assert_eq!(parsed.timestamp, None);
        assert_eq!(parsed.body, line);
    }

    #[test]
    fn error_body_follows_the_marker() {
        let parsed =
            parse(b"2024-05-01T10:00:05.0000000Z ##[error]Process completed with exit code 1.");
        assert_eq!(parsed.marker, LineMarker::Error);
        assert_eq!(parsed.body, b"Process completed with exit code 1.");
    }

    #[test]
    fn run_groups_and_post_cleanup_start_steps() {
        let starts = |line: &[u8]| ActionsLogDialect.is_step_start(&parse(line));
        assert!(starts(
            b"2024-05-01T10:00:05.0Z ##[group]Run actions/checkout@v4"
        ));
        assert!(starts(b"2024-05-01T10:00:05.0Z Post job cleanup."));
        assert!(!starts(
            b"2024-05-01T10:00:05.0Z ##[group]Runner Image Provisioner"
        ));
        assert!(!starts(b"2024-05-01T10:00:05.0Z Run actions/checkout@v4"));
        assert!(!starts(b"2024-05-01T10:00:05.0Z ##[error]Run failed"));
    }
}
