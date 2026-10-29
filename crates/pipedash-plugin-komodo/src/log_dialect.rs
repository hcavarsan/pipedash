use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

use crate::types::UpdateLog;

const TAGS: [(u8, LineMarker); 4] = [
    (b'.', LineMarker::Plain),
    (b'$', LineMarker::Command),
    (b'!', LineMarker::Error),
    (b'?', LineMarker::Warning),
];

pub struct KomodoLogDialect;

impl LogDialect for KomodoLogDialect {
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

pub(crate) fn stage_lines(log: &UpdateLog) -> impl Iterator<Item = (LineMarker, &str)> {
    let stderr = if log.success {
        LineMarker::Warning
    } else {
        LineMarker::Error
    };
    [
        (LineMarker::Command, log.command.as_str()),
        (LineMarker::Plain, log.stdout.as_str()),
        (stderr, log.stderr.as_str()),
    ]
    .into_iter()
    .flat_map(|(marker, text)| text.lines().map(move |line| (marker, line)))
}

pub(crate) fn push_line(out: &mut Vec<u8>, marker: LineMarker, line: &str) {
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
    use super::*;

    fn parse(line: &[u8]) -> ParsedLine<'_> {
        KomodoLogDialect.parse(line)
    }

    fn log(success: bool, command: &str, stdout: &str, stderr: &str) -> UpdateLog {
        UpdateLog {
            stage: "Compose Up".to_string(),
            command: command.to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            success,
            start_ts: 0,
            end_ts: 0,
        }
    }

    fn encode(log: &UpdateLog) -> Vec<u8> {
        let mut out = Vec::new();
        for (marker, line) in stage_lines(log) {
            push_line(&mut out, marker, line);
        }
        out
    }

    fn parsed_lines(encoded: &[u8]) -> Vec<(LineMarker, String)> {
        encoded
            .strip_suffix(b"\n")
            .into_iter()
            .flat_map(|text| text.split(|b| *b == b'\n'))
            .map(|line| {
                let parsed = parse(line);
                (
                    parsed.marker,
                    String::from_utf8_lossy(parsed.body).into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn every_tag_maps_to_its_marker_and_strips_itself() {
        let cases: [(&[u8], LineMarker, &[u8]); 4] = [
            (b". pulling image", LineMarker::Plain, b"pulling image"),
            (
                b"$ docker compose up -d",
                LineMarker::Command,
                b"$ docker compose up -d",
            ),
            (b"! no such service", LineMarker::Error, b"no such service"),
            (b"? network exists", LineMarker::Warning, b"network exists"),
        ];
        for (line, marker, body) in cases {
            let parsed = parse(line);
            assert_eq!(parsed.marker, marker, "{line:?}");
            assert_eq!(parsed.body, body, "{line:?}");
            assert_eq!(parsed.timestamp, None, "{line:?}");
        }
    }

    #[test]
    fn tagged_lines_without_text_keep_their_marker() {
        for (line, marker) in [
            (b". ".as_slice(), LineMarker::Plain),
            (b"! ", LineMarker::Error),
            (b"? ", LineMarker::Warning),
        ] {
            let parsed = parse(line);
            assert_eq!(parsed.marker, marker, "{line:?}");
            assert_eq!(parsed.body, b"", "{line:?}");
        }
    }

    #[test]
    fn lines_without_a_known_tag_and_a_space_stay_plain() {
        let lines: [&[u8]; 8] = [
            b"",
            b"plain output",
            b"!no space",
            b"$docker",
            b"x unknown tag",
            b"$",
            b"!",
            b"  indented",
        ];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
        }
    }

    #[test]
    fn stderr_is_an_error_in_a_failed_stage_and_a_warning_in_a_successful_one() {
        let failed = encode(&log(false, "docker compose up", "out", "boom"));
        let passed = encode(&log(true, "docker compose up", "out", "boom"));

        assert_eq!(
            parsed_lines(&failed),
            [
                (LineMarker::Command, "$ docker compose up".to_string()),
                (LineMarker::Plain, "out".to_string()),
                (LineMarker::Error, "boom".to_string()),
            ]
        );
        assert_eq!(
            parsed_lines(&passed),
            [
                (LineMarker::Command, "$ docker compose up".to_string()),
                (LineMarker::Plain, "out".to_string()),
                (LineMarker::Warning, "boom".to_string()),
            ]
        );
    }

    #[test]
    fn every_line_of_a_multi_line_text_carries_its_tag() {
        let encoded = encode(&log(
            false,
            "set -e\ndocker compose up",
            "a\n\nb\n",
            "x\r\ny",
        ));

        assert_eq!(
            encoded,
            b"$ set -e\n$ docker compose up\n. a\n. \n. b\n! x\n! y\n"
        );
    }

    #[test]
    fn empty_command_and_streams_write_nothing() {
        assert!(encode(&log(true, "", "", "")).is_empty());
    }

    #[test]
    fn a_tagged_line_that_looks_like_output_of_another_stream_stays_in_its_stream() {
        let encoded = encode(&log(false, "", "! not an error", ""));

        assert_eq!(
            parsed_lines(&encoded),
            [(LineMarker::Plain, "! not an error".to_string())]
        );
    }

    #[test]
    fn carriage_returns_keep_the_tag_on_the_part_that_stays_visible() {
        let mut out = Vec::new();
        push_line(&mut out, LineMarker::Error, "10%\r55%\r100%\r");
        assert_eq!(out, b"! 10%\r! 55%\r! 100%\n");

        let line = out.strip_suffix(b"\n").unwrap();
        let visible = line
            .rsplit(|b| *b == b'\r')
            .find(|segment| !segment.is_empty())
            .unwrap();
        let parsed = parse(visible);
        assert_eq!(parsed.marker, LineMarker::Error);
        assert_eq!(parsed.body, b"100%");
    }
}
