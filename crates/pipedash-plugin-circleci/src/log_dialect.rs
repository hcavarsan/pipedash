use chrono::{
    DateTime,
    SecondsFormat,
    Utc,
};
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

use crate::types::OutputMessage;

const NO_TIMESTAMP: &str = "-";

const TAGS: [(u8, LineMarker); 3] = [
    (b'.', LineMarker::Plain),
    (b'#', LineMarker::SectionStart { open: false }),
    (b'+', LineMarker::SectionStart { open: true }),
];

pub(crate) struct CircleCILogDialect;

impl LogDialect for CircleCILogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let plain = ParsedLine {
            timestamp: None,
            marker: LineMarker::Plain,
            body: line,
        };
        let Some((tag, rest)) = line.split_first() else {
            return plain;
        };
        let Some(marker) = TAGS
            .iter()
            .find(|(known, _)| known == tag)
            .map(|(_, marker)| *marker)
        else {
            return plain;
        };
        let Some(space) = rest.iter().position(|b| *b == b' ') else {
            return plain;
        };
        let timestamp = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|stamp| DateTime::parse_from_rfc3339(stamp).ok())
            .map(|stamp| stamp.with_timezone(&Utc));
        ParsedLine {
            timestamp,
            marker,
            body: &rest[space + 1..],
        }
    }
}

pub(crate) struct Section<'a> {
    pub name: String,
    pub failed: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub output: &'a [OutputMessage],
}

pub(crate) fn render(sections: &[Section<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for section in sections {
        let marker = LineMarker::SectionStart {
            open: section.failed,
        };
        push_line(&mut out, marker, section.started_at, &section.name);
        let mut line = String::new();
        let mut line_at = None;
        for message in section.output {
            for piece in message.message.split_inclusive('\n') {
                if line.is_empty() {
                    line_at = message.time;
                }
                line.push_str(piece);
                if line.ends_with('\n') {
                    push_line(&mut out, LineMarker::Plain, line_at, &line);
                    line.clear();
                }
            }
        }
        if !line.is_empty() {
            push_line(&mut out, LineMarker::Plain, line_at, &line);
        }
    }
    out
}

fn push_line(out: &mut Vec<u8>, marker: LineMarker, at: Option<DateTime<Utc>>, text: &str) {
    let tag = TAGS
        .iter()
        .find(|(_, known)| *known == marker)
        .map_or(b'.', |(tag, _)| *tag);
    let stamp = at.map(|at| at.to_rfc3339_opts(SecondsFormat::AutoSi, true));
    let stamp = stamp.as_deref().unwrap_or(NO_TIMESTAMP);
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    for (index, segment) in text.split('\r').enumerate() {
        if index > 0 {
            out.push(b'\r');
        }
        out.push(tag);
        out.extend_from_slice(stamp.as_bytes());
        out.push(b' ');
        out.extend_from_slice(segment.as_bytes());
    }
    out.push(b'\n');
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn messages(value: serde_json::Value) -> Vec<OutputMessage> {
        serde_json::from_value(value).unwrap()
    }

    fn parsed(rendered: &[u8]) -> Vec<(LineMarker, Option<DateTime<Utc>>, String)> {
        rendered
            .strip_suffix(b"\n")
            .unwrap()
            .split(|b| *b == b'\n')
            .map(|line| {
                let visible = line.rsplit(|b| *b == b'\r').next().unwrap();
                let parsed = CircleCILogDialect.parse(visible);
                (
                    parsed.marker,
                    parsed.timestamp,
                    String::from_utf8_lossy(parsed.body).into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn steps_become_sections_and_lines_join_across_output_chunks() {
        let output = messages(json!([
            {"message": "Cloning into 'repo'...\r\nrem", "time": "2026-01-01T00:00:01Z", "type": "out"},
            {"message": "ote: done\r\n", "time": "2026-01-01T00:00:02Z", "type": "err"},
            {"message": "no newline at end", "time": "2026-01-01T00:00:03Z", "type": "out"}
        ]));
        let started = "2026-01-01T00:00:00Z".parse().ok();
        let rendered = render(&[
            Section {
                name: "Checkout code".into(),
                failed: false,
                started_at: started,
                output: &output,
            },
            Section {
                name: "Run tests".into(),
                failed: true,
                started_at: None,
                output: &[],
            },
        ]);

        let at = |second: u32| {
            Some(
                format!("2026-01-01T00:00:0{second}Z")
                    .parse::<DateTime<Utc>>()
                    .unwrap(),
            )
        };
        assert_eq!(
            parsed(&rendered),
            [
                (
                    LineMarker::SectionStart { open: false },
                    started,
                    "Checkout code".to_string()
                ),
                (
                    LineMarker::Plain,
                    at(1),
                    "Cloning into 'repo'...".to_string()
                ),
                (LineMarker::Plain, at(1), "remote: done".to_string()),
                (LineMarker::Plain, at(3), "no newline at end".to_string()),
                (
                    LineMarker::SectionStart { open: true },
                    None,
                    "Run tests".to_string()
                ),
            ]
        );
    }

    #[test]
    fn progress_carriage_returns_keep_the_tag_on_the_visible_part() {
        let output = messages(json!([{"message": "10%\r55%\r100%\r\n", "type": "out"}]));
        let rendered = render(&[Section {
            name: "Download".into(),
            failed: false,
            started_at: None,
            output: &output,
        }]);

        assert_eq!(rendered, b"#- Download\n.- 10%\r.- 55%\r.- 100%\n");
    }

    #[test]
    fn output_cannot_forge_a_section() {
        let output = messages(json!([{"message": "+- fake section\n", "type": "out"}]));
        let rendered = render(&[Section {
            name: "Run".into(),
            failed: false,
            started_at: None,
            output: &output,
        }]);

        assert_eq!(
            parsed(&rendered)[1],
            (LineMarker::Plain, None, "+- fake section".to_string())
        );
    }

    #[test]
    fn untagged_lines_stay_plain() {
        for line in [b"".as_slice(), b"plain", b"#nospace", b"x- unknown"] {
            let parsed = CircleCILogDialect.parse(line);
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
        }
    }
}
