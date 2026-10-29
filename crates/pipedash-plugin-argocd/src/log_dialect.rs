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

const NO_TIMESTAMP: &str = "-";

const TAGS: [(u8, LineMarker); 5] = [
    (b'.', LineMarker::Plain),
    (b'!', LineMarker::Error),
    (b'?', LineMarker::Warning),
    (b'#', LineMarker::SectionStart { open: false }),
    (b'+', LineMarker::SectionStart { open: true }),
];

pub struct ArgocdLogDialect;

impl LogDialect for ArgocdLogDialect {
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

    fn infers_errors(&self) -> bool {
        false
    }
}

pub(crate) fn push_line(out: &mut Vec<u8>, marker: LineMarker, stamp: Option<&str>, text: &str) {
    let tag = TAGS
        .iter()
        .find(|(_, known)| *known == marker)
        .map_or(b'.', |(tag, _)| *tag);
    for line in text.split("\r\n").flat_map(|line| line.split(['\n', '\r'])) {
        out.push(tag);
        out.extend_from_slice(stamp.unwrap_or(NO_TIMESTAMP).as_bytes());
        out.push(b' ');
        out.extend_from_slice(line.as_bytes());
        out.push(b'\n');
    }
}

pub(crate) fn push_line_at(
    out: &mut Vec<u8>, marker: LineMarker, at: Option<DateTime<Utc>>, text: &str,
) {
    let stamp = at.map(|at| at.to_rfc3339_opts(SecondsFormat::AutoSi, true));
    push_line(out, marker, stamp.as_deref(), text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> ParsedLine<'_> {
        ArgocdLogDialect.parse(line)
    }

    #[test]
    fn written_lines_parse_back_with_marker_timestamp_and_text() {
        let at = DateTime::parse_from_rfc3339("2026-10-27T10:00:00.123456789Z")
            .unwrap()
            .with_timezone(&Utc);
        let cases = [
            (LineMarker::Plain, Some(at)),
            (LineMarker::Error, None),
            (LineMarker::Warning, Some(at)),
            (LineMarker::SectionStart { open: false }, None),
            (LineMarker::SectionStart { open: true }, Some(at)),
        ];
        for (marker, stamp) in cases {
            let mut out = Vec::new();
            push_line_at(&mut out, marker, stamp, "level=info msg=\"ready\"");
            let line = out.strip_suffix(b"\n").unwrap();
            let parsed = parse(line);
            assert_eq!(parsed.marker, marker, "{line:?}");
            assert_eq!(parsed.timestamp, stamp, "{line:?}");
            assert_eq!(parsed.body, b"level=info msg=\"ready\"", "{line:?}");
        }
    }

    #[test]
    fn multi_line_text_keeps_the_marker_on_every_line() {
        let mut out = Vec::new();
        push_line(
            &mut out,
            LineMarker::Error,
            None,
            "apply failed\nfield is immutable",
        );
        assert_eq!(out, b"!- apply failed\n!- field is immutable\n");
    }

    #[test]
    fn carriage_returns_start_new_tagged_lines() {
        let mut out = Vec::new();
        push_line(
            &mut out,
            LineMarker::Error,
            None,
            "failed\rmore detail\r\n+- fake section",
        );
        assert_eq!(
            out,
            b"!- failed\n!- more detail\n!- +- fake section\n".as_slice()
        );
    }

    #[test]
    fn container_output_cannot_forge_a_marker() {
        let mut out = Vec::new();
        push_line(
            &mut out,
            LineMarker::Plain,
            Some("2026-10-27T10:00:00Z"),
            "+- fake section",
        );
        let parsed = parse(out.strip_suffix(b"\n").unwrap());
        assert_eq!(parsed.marker, LineMarker::Plain);
        assert_eq!(parsed.body, b"+- fake section");
    }

    #[test]
    fn untagged_lines_stay_plain_and_whole() {
        for line in [b"".as_slice(), b"plain text", b"!no-space", b"x- body"] {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.timestamp, None, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
        }
    }
}
