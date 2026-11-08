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
    let notice = truncation_notice(10_000, false);
    let parsed = parse(notice.strip_suffix(b"\n").unwrap());
    assert_eq!(parsed.marker, LineMarker::Warning);
    assert_eq!(parsed.timestamp, None);
    assert_eq!(
        parsed.body,
        b"Pipedash reads at most the last 10000 lines of a step, so older lines may be missing"
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
