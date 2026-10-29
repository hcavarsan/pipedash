use pipedash_plugin_actions::ActionsLogDialect;
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

pub struct GiteaLogDialect;

impl LogDialect for GiteaLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let parsed = ActionsLogDialect.parse(line);
        if parsed.marker != LineMarker::Plain {
            return parsed;
        }
        match workflow_command(parsed.body) {
            Some((marker, body)) => ParsedLine {
                marker,
                body,
                ..parsed
            },
            None => parsed,
        }
    }
}

fn workflow_command(body: &[u8]) -> Option<(LineMarker, &[u8])> {
    let rest = body.strip_prefix(b"::")?;
    let end = rest.windows(2).position(|pair| pair == b"::")?;
    let name = rest[..end].split(|b| *b == b' ').next()?;
    let marker = match name {
        b"group" => LineMarker::GroupStart,
        b"endgroup" => LineMarker::GroupEnd,
        b"error" => LineMarker::Error,
        b"warning" => LineMarker::Warning,
        b"notice" => LineMarker::Notice,
        b"debug" => LineMarker::Debug,
        _ => return None,
    };
    Some((marker, &rest[end + 2..]))
}

#[cfg(test)]
mod tests {
    use chrono::{
        DateTime,
        Utc,
    };

    use super::*;

    fn parse(line: &[u8]) -> ParsedLine<'_> {
        GiteaLogDialect.parse(line)
    }

    #[test]
    fn recorded_gitea_line_splits_into_timestamp_and_text() {
        let parsed = parse(b"2026-10-28T10:15:30.1234567Z + go test ./...");
        assert_eq!(parsed.marker, LineMarker::Plain);
        assert_eq!(parsed.body, b"+ go test ./...");
        assert_eq!(
            parsed.timestamp,
            Some(
                "2026-10-28T10:15:30.1234567Z"
                    .parse::<DateTime<Utc>>()
                    .unwrap()
            )
        );
    }

    #[test]
    fn group_and_endgroup_commands_open_and_close_a_section() {
        let group = parse(b"2026-10-28T10:15:31.0000000Z ::group::Install dependencies");
        assert_eq!(group.marker, LineMarker::GroupStart);
        assert_eq!(group.body, b"Install dependencies");
        assert!(group.timestamp.is_some());

        let end = parse(b"2026-10-28T10:15:32.0000000Z ::endgroup::");
        assert_eq!(end.marker, LineMarker::GroupEnd);
        assert_eq!(end.body, b"");
    }

    #[test]
    fn annotation_commands_become_severity_markers() {
        let cases: [(&[u8], LineMarker, &[u8]); 4] = [
            (b"::error::Build failed", LineMarker::Error, b"Build failed"),
            (
                b"::warning::Cache missed",
                LineMarker::Warning,
                b"Cache missed",
            ),
            (b"::notice::Deployed", LineMarker::Notice, b"Deployed"),
            (
                b"::debug::cwd=/workspace",
                LineMarker::Debug,
                b"cwd=/workspace",
            ),
        ];
        for (text, marker, body) in cases {
            let line = [b"2026-10-28T10:15:33.0000000Z ".as_slice(), text].concat();
            let parsed = parse(&line);
            assert_eq!(parsed.marker, marker, "{}", String::from_utf8_lossy(text));
            assert!(parsed.timestamp.is_some());
            assert_eq!(parsed.body, body);
        }
    }

    #[test]
    fn annotation_parameters_are_dropped_from_the_body() {
        let parsed = parse(
            b"2026-10-28T10:15:34.0000000Z ::error file=app.js,line=3,col=7::Missing semicolon",
        );
        assert_eq!(parsed.marker, LineMarker::Error);
        assert_eq!(parsed.body, b"Missing semicolon");
    }

    #[test]
    fn github_style_markers_still_work() {
        let parsed = parse(b"2026-10-28T10:15:35.0000000Z ##[group]Run make");
        assert_eq!(parsed.marker, LineMarker::GroupStart);
        assert_eq!(parsed.body, b"Run make");

        let parsed = parse(b"2026-10-28T10:15:36.0000000Z ##[error]exit code 2");
        assert_eq!(parsed.marker, LineMarker::Error);
        assert_eq!(parsed.body, b"exit code 2");
    }

    #[test]
    fn other_workflow_commands_and_unterminated_commands_stay_plain() {
        for text in [
            &b"::set-output name=sha::abc123"[..],
            b"::add-mask::***",
            b"::group no closing colons",
            b"::groupx::name",
            b"echo ::group::inside a sentence",
        ] {
            let line = [b"2026-10-28T10:15:37.0000000Z ".as_slice(), text].concat();
            let parsed = parse(&line);
            assert_eq!(
                parsed.marker,
                LineMarker::Plain,
                "{}",
                String::from_utf8_lossy(text)
            );
            assert_eq!(parsed.body, text);
        }
    }

    #[test]
    fn line_without_timestamp_still_reads_its_command() {
        let parsed = parse(b"::group::Set up job");
        assert_eq!(parsed.timestamp, None);
        assert_eq!(parsed.marker, LineMarker::GroupStart);
        assert_eq!(parsed.body, b"Set up job");
    }

    #[test]
    fn no_line_starts_a_step() {
        for text in [
            &b"::group::Run actions/checkout@v4"[..],
            b"Post job cleanup.",
        ] {
            let parsed = parse(text);
            assert!(!GiteaLogDialect.is_step_start(&parsed));
        }
    }
}
