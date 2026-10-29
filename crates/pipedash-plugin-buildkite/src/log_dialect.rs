use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const TIMESTAMP_START: &[u8] = b"\x1b_bk;t=";

const SECTIONS: [(&[u8], LineMarker); 4] = [
    (b"^^^ +++", LineMarker::OpenSection),
    (b"--- ", LineMarker::SectionStart { open: false }),
    (b"~~~ ", LineMarker::SectionStart { open: false }),
    (b"+++ ", LineMarker::SectionStart { open: true }),
];

const MESSAGES: [(&[u8], LineMarker); 3] = [
    ("🚨 Error: ".as_bytes(), LineMarker::Error),
    ("⚠️ Warning: ".as_bytes(), LineMarker::Warning),
    (b"$", LineMarker::Command),
];

pub struct BuildkiteLogDialect;

impl LogDialect for BuildkiteLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let (timestamp, body) = split_timestamp(line);
        let text = skip_sgr(body);
        for (prefix, marker) in SECTIONS {
            if let Some(rest) = text.strip_prefix(prefix) {
                return ParsedLine {
                    timestamp,
                    marker,
                    body: rest.strip_prefix(b" ").unwrap_or(rest),
                };
            }
        }
        let marker = MESSAGES
            .iter()
            .find(|(prefix, marker)| {
                text.strip_prefix(*prefix).is_some_and(|rest| {
                    *marker != LineMarker::Command || skip_sgr(rest).starts_with(b" ")
                })
            })
            .map_or(LineMarker::Plain, |(_, marker)| *marker);
        ParsedLine {
            timestamp,
            marker,
            body,
        }
    }

    fn infers_errors(&self) -> bool {
        false
    }
}

fn split_timestamp(line: &[u8]) -> (Option<DateTime<Utc>>, &[u8]) {
    let Some(rest) = line.strip_prefix(TIMESTAMP_START) else {
        return (None, line);
    };
    let Some(end) = rest.iter().position(|b| *b == b'\x07') else {
        return (None, line);
    };
    std::str::from_utf8(&rest[..end])
        .ok()
        .and_then(|ms| ms.parse::<i64>().ok())
        .and_then(DateTime::from_timestamp_millis)
        .map_or((None, line), |ts| (Some(ts), &rest[end + 1..]))
}

fn skip_sgr(mut text: &[u8]) -> &[u8] {
    while let Some(rest) = text.strip_prefix(b"\x1b[") {
        let Some(end) = rest.iter().position(|b| *b == b'm') else {
            break;
        };
        if !rest[..end].iter().all(|b| b.is_ascii_digit() || *b == b';') {
            break;
        }
        text = &rest[end + 1..];
    }
    text
}

#[cfg(test)]
mod tests {
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
                "\x1b_bk;t=1\x07\x1b[31m🚨 Error: The command exited with status 1\x1b[0m"
                    .as_bytes(),
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
}
