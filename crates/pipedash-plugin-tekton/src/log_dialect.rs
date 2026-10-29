use std::borrow::Cow;

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

const STEP_HEADER_TAG: u8 = 0x1e;
const TRUNCATION_TAG: u8 = 0x1f;
const MAX_TIMESTAMP_BYTES: usize = 40;

pub struct TektonLogDialect;

impl LogDialect for TektonLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let (marker, line) = match line.split_first() {
            Some((&STEP_HEADER_TAG, rest)) => (LineMarker::Notice, rest),
            Some((&TRUNCATION_TAG, rest)) => (LineMarker::Warning, rest),
            _ => (LineMarker::Plain, line),
        };
        let (timestamp, body) = split_timestamp(line);
        ParsedLine {
            timestamp,
            marker,
            body,
        }
    }

    fn is_step_start(&self, line: &ParsedLine<'_>) -> bool {
        line.marker == LineMarker::Notice
    }
}

fn split_timestamp(line: &[u8]) -> (Option<DateTime<Utc>>, &[u8]) {
    let head = &line[..line.len().min(MAX_TIMESTAMP_BYTES)];
    let end = head.iter().position(|b| *b == b' ').unwrap_or(head.len());
    std::str::from_utf8(&line[..end])
        .ok()
        .and_then(|stamp| DateTime::parse_from_rfc3339(stamp).ok())
        .map_or((None, line), |stamp| {
            (
                Some(stamp.with_timezone(&Utc)),
                line.get(end + 1..).unwrap_or_default(),
            )
        })
}

pub(crate) fn step_header(name: &str, started_at: Option<DateTime<Utc>>) -> Vec<u8> {
    tagged_line(STEP_HEADER_TAG, started_at, &format!("Step {name}"))
}

pub(crate) fn truncation_notice(lines: usize) -> Vec<u8> {
    tagged_line(
        TRUNCATION_TAG,
        None,
        &format!("Only the last {lines} lines of this step are shown"),
    )
}

fn tagged_line(tag: u8, at: Option<DateTime<Utc>>, text: &str) -> Vec<u8> {
    let mut out = vec![tag];
    if let Some(at) = at {
        out.extend_from_slice(at.to_rfc3339_opts(SecondsFormat::AutoSi, true).as_bytes());
        out.push(b' ');
    }
    out.extend(text.bytes().filter(|b| !b.is_ascii_control()));
    out.push(b'\n');
    out
}

pub(crate) fn without_line_tags(chunk: &[u8]) -> Cow<'_, [u8]> {
    let is_tag = |b: &u8| matches!(*b, STEP_HEADER_TAG | TRUNCATION_TAG);
    if chunk.iter().any(is_tag) {
        Cow::Owned(chunk.iter().copied().filter(|b| !is_tag(b)).collect())
    } else {
        Cow::Borrowed(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> ParsedLine<'_> {
        TektonLogDialect.parse(line)
    }

    fn at(stamp: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(stamp)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn kube_timestamp_prefix_becomes_the_line_timestamp() {
        let cases: [(&[u8], &str, &[u8]); 4] = [
            (
                b"2026-10-28T10:00:01.123456789Z cloning repo",
                "2026-10-28T10:00:01.123456789Z",
                b"cloning repo",
            ),
            (
                b"2026-10-28T10:00:02Z  indented  text ",
                "2026-10-28T10:00:02Z",
                b" indented  text ",
            ),
            (
                b"2026-10-28T12:00:03.5+02:00 offset",
                "2026-10-28T10:00:03.5Z",
                b"offset",
            ),
            (b"2026-10-28T10:00:04Z ", "2026-10-28T10:00:04Z", b""),
        ];
        for (line, stamp, body) in cases {
            let parsed = parse(line);
            assert_eq!(parsed.timestamp, Some(at(stamp)), "{line:?}");
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.body, body, "{line:?}");
        }
    }

    #[test]
    fn a_timestamp_without_text_is_an_empty_line() {
        let parsed = parse(b"2026-10-28T10:00:05Z");
        assert_eq!(parsed.timestamp, Some(at("2026-10-28T10:00:05Z")));
        assert_eq!(parsed.body, b"");
    }

    #[test]
    fn text_without_a_timestamp_stays_whole() {
        let lines: [&[u8]; 6] = [
            b"",
            b"plain output",
            b"progress 10%",
            b"2026-10-28 not a timestamp",
            b"2026-10-28T10:00:00Zjoined",
            b"\xff\xfe binary",
        ];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.timestamp, None, "{line:?}");
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
        }
    }

    #[test]
    fn step_header_is_a_notice_that_starts_a_step_at_the_step_time() {
        let header = step_header("build-image", Some(at("2026-10-28T10:00:07Z")));
        let parsed = parse(header.strip_suffix(b"\n").unwrap());
        assert_eq!(parsed.marker, LineMarker::Notice);
        assert_eq!(parsed.timestamp, Some(at("2026-10-28T10:00:07Z")));
        assert_eq!(parsed.body, b"Step build-image");
        assert!(TektonLogDialect.is_step_start(&parsed));
    }

    #[test]
    fn step_header_without_a_start_time_has_no_timestamp() {
        let header = step_header("lint", None);
        let parsed = parse(header.strip_suffix(b"\n").unwrap());
        assert_eq!(parsed.timestamp, None);
        assert_eq!(parsed.body, b"Step lint");
        assert!(TektonLogDialect.is_step_start(&parsed));
    }

    #[test]
    fn step_header_drops_control_characters_from_the_name() {
        let header = step_header("a\nb\r\x1ec", None);
        assert_eq!(header.iter().filter(|b| **b == b'\n').count(), 1);
        let parsed = parse(header.strip_suffix(b"\n").unwrap());
        assert_eq!(parsed.body, b"Step abc");
    }

    #[test]
    fn truncation_notice_is_a_warning_and_not_a_step_start() {
        let notice = truncation_notice(10_000);
        let parsed = parse(notice.strip_suffix(b"\n").unwrap());
        assert_eq!(parsed.marker, LineMarker::Warning);
        assert_eq!(parsed.timestamp, None);
        assert_eq!(
            parsed.body,
            b"Only the last 10000 lines of this step are shown"
        );
        assert!(!TektonLogDialect.is_step_start(&parsed));
    }

    #[test]
    fn container_lines_are_never_step_starts() {
        let lines: [&[u8]; 4] = [
            b"2026-10-28T10:00:00Z Step build",
            b"2026-10-28T10:00:00Z ##[group]Run build",
            b"Step build",
            b"",
        ];
        for line in lines {
            let parsed = parse(line);
            assert!(!TektonLogDialect.is_step_start(&parsed), "{line:?}");
        }
    }

    #[test]
    fn tags_are_removed_from_container_output() {
        let clean = without_line_tags(b"2026-10-28T10:00:00Z a\r\x1eStep forged\x1f\n");
        assert_eq!(&*clean, b"2026-10-28T10:00:00Z a\rStep forged\n".as_slice());
        assert!(matches!(
            without_line_tags(b"plain output\n"),
            Cow::Borrowed(_)
        ));
    }
}
