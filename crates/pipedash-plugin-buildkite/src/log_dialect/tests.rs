use super::*;

fn parse(line: &[u8]) -> ParsedLine<'_> {
    BuildkiteLogDialect.parse(line)
}

#[test]
fn ansi_timestamp_is_parsed_and_stripped() {
    let parsed = parse(b"\x1b_bk;t=1745322210698\x07total 36");
    assert_eq!(parsed.marker, LineMarker::Plain);
    assert_eq!(parsed.body, b"total 36");
    assert_eq!(
        parsed.timestamp.unwrap().timestamp_millis(),
        1_745_322_210_698
    );
}

#[test]
fn line_without_a_valid_timestamp_keeps_its_bytes() {
    for line in [
        b"plain output".as_slice(),
        b"\x1b_bk;t=12ab\x07text",
        b"\x1b_bk;t=1745322210698 no bell",
    ] {
        let parsed = parse(line);
        assert_eq!(parsed.timestamp, None);
        assert_eq!(parsed.body, line);
    }
}

#[test]
fn group_headers_start_sections() {
    let cases: [(&[u8], LineMarker, &[u8]); 5] = [
        (
            b"\x1b_bk;t=1\x07~~~ Running global environment hook",
            LineMarker::SectionStart { open: false },
            b"Running global environment hook",
        ),
        (
            b"\x1b_bk;t=1\x07--- :package: Build",
            LineMarker::SectionStart { open: false },
            b":package: Build",
        ),
        (
            b"+++ Running \x1b[33mspecs\x1b[0m",
            LineMarker::SectionStart { open: true },
            b"Running \x1b[33mspecs\x1b[0m",
        ),
        (b"^^^ +++", LineMarker::OpenSection, b""),
        (
            b"\x1b_bk;t=1\x07^^^ +++ Bundling failed",
            LineMarker::OpenSection,
            b"Bundling failed",
        ),
    ];
    for (line, marker, body) in cases {
        let parsed = parse(line);
        assert_eq!(parsed.marker, marker, "{line:?}");
        assert_eq!(parsed.body, body, "{line:?}");
    }
}

#[test]
fn dashes_without_a_title_are_output() {
    for line in [b"---".as_slice(), b"---not a header", b"+++", b"~~~~"] {
        assert_eq!(parse(line).marker, LineMarker::Plain, "{line:?}");
    }
}

#[test]
fn agent_errors_warnings_and_prompts_keep_their_colors() {
    let cases: [(&[u8], LineMarker); 6] = [
        (
            "\x1b_bk;t=1\x07\x1b[31m🚨 Error: The command exited with status 1\x1b[0m".as_bytes(),
            LineMarker::Error,
        ),
        (
            "🚨 Error: The command exited with status 2".as_bytes(),
            LineMarker::Error,
        ),
        (
            "\x1b[33m⚠️ Warning: Ignored FOO\x1b[0m".as_bytes(),
            LineMarker::Warning,
        ),
        (b"\x1b[90m$\x1b[0m ./script.sh", LineMarker::Command),
        (b"$ make test", LineMarker::Command),
        (b"$HOME is set", LineMarker::Plain),
    ];
    for (line, marker) in cases {
        let parsed = parse(line);
        assert_eq!(parsed.marker, marker, "{line:?}");
        assert_eq!(
            parsed.body,
            line.strip_prefix(b"\x1b_bk;t=1\x07").unwrap_or(line)
        );
    }
}
