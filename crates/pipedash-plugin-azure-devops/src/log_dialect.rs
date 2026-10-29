use pipedash_plugin_actions::ActionsLogDialect;
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const SECTION: &[u8] = b"##[section]";
const TASK_START: &[u8] = b"Starting: ";
const TASK_END: &[u8] = b"Finishing: ";

pub struct AzureDevOpsLogDialect;

impl LogDialect for AzureDevOpsLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let parsed = ActionsLogDialect.parse(line);
        match (parsed.marker, parsed.body.strip_prefix(SECTION)) {
            (LineMarker::Plain, Some(title)) => ParsedLine {
                marker: if title.starts_with(TASK_END) {
                    LineMarker::Plain
                } else {
                    LineMarker::SectionStart { open: false }
                },
                body: title,
                ..parsed
            },
            _ => parsed,
        }
    }

    fn is_step_start(&self, line: &ParsedLine<'_>) -> bool {
        line.marker == (LineMarker::SectionStart { open: false })
            && line.body.starts_with(TASK_START)
    }

    fn infers_errors(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: &str = "2024-01-01T10:00:00.1234567Z";

    fn parse(line: &str) -> ParsedLine<'_> {
        AzureDevOpsLogDialect.parse(line.as_bytes())
    }

    fn stamped(text: &str) -> String {
        format!("{STAMP} {text}")
    }

    #[test]
    fn timestamp_has_seven_fraction_digits_and_a_z_suffix() {
        let line = stamped("plain output");
        let parsed = parse(&line);

        assert_eq!(
            parsed.timestamp.unwrap().to_rfc3339(),
            "2024-01-01T10:00:00.123456700+00:00"
        );
        assert_eq!(parsed.marker, LineMarker::Plain);
        assert_eq!(parsed.body, b"plain output");
    }

    #[test]
    fn line_without_a_timestamp_keeps_its_text() {
        let parsed = parse("##[error]no stamp");

        assert_eq!(parsed.timestamp, None);
        assert_eq!(parsed.marker, LineMarker::Error);
        assert_eq!(parsed.body, b"no stamp");
    }

    #[test]
    fn byte_order_mark_at_the_start_of_a_page_is_dropped() {
        let line = format!("\u{feff}{}", stamped("##[section]Starting: Initialize job"));
        let parsed = parse(&line);

        assert!(parsed.timestamp.is_some());
        assert_eq!(parsed.marker, LineMarker::SectionStart { open: false });
        assert_eq!(parsed.body, b"Starting: Initialize job");
    }

    #[test]
    fn section_starts_a_collapsed_section_titled_by_the_text() {
        for title in [
            "Starting: Initialize job",
            "Async Command Start: DetectDockerContainer",
        ] {
            let line = stamped(&format!("##[section]{title}"));
            let parsed = parse(&line);

            assert_eq!(parsed.marker, LineMarker::SectionStart { open: false });
            assert_eq!(parsed.body, title.as_bytes());
            assert!(parsed.timestamp.is_some());
        }
    }

    #[test]
    fn finishing_a_task_stays_a_plain_line_inside_its_section() {
        let line = stamped("##[section]Finishing: Checkout acme/app@main to s");
        let parsed = parse(&line);

        assert_eq!(parsed.marker, LineMarker::Plain);
        assert_eq!(parsed.body, b"Finishing: Checkout acme/app@main to s");
        assert!(parsed.timestamp.is_some());
    }

    #[test]
    fn group_and_endgroup_open_and_close_a_group() {
        let open = stamped("##[group]Run tests");
        let close = stamped("##[endgroup]");

        let opened = parse(&open);
        let closed = parse(&close);

        assert_eq!(opened.marker, LineMarker::GroupStart);
        assert_eq!(opened.body, b"Run tests");
        assert_eq!(closed.marker, LineMarker::GroupEnd);
        assert_eq!(closed.body, b"");
    }

    #[test]
    fn error_warning_command_and_debug_keep_their_text() {
        let cases = [
            ("##[error]Bash exited with code '1'.", LineMarker::Error),
            ("##[warning]Deprecated task", LineMarker::Warning),
            ("##[command]git fetch --prune", LineMarker::Command),
            ("##[debug]Evaluating: variables", LineMarker::Debug),
        ];
        for (text, marker) in cases {
            let line = stamped(text);
            let parsed = parse(&line);

            assert_eq!(parsed.marker, marker, "{text}");
            assert_eq!(
                parsed.body,
                text.split_once(']').unwrap().1.as_bytes(),
                "{text}"
            );
            assert!(parsed.timestamp.is_some(), "{text}");
        }
    }

    #[test]
    fn task_output_commands_without_the_hashes_are_commands() {
        let line = stamped("[command]/usr/bin/git version");
        let parsed = parse(&line);

        assert_eq!(parsed.marker, LineMarker::Command);
        assert_eq!(parsed.body, b"/usr/bin/git version");
    }

    #[test]
    fn only_a_starting_section_starts_a_step() {
        let starts = |text: &str| AzureDevOpsLogDialect.is_step_start(&parse(&stamped(text)));

        assert!(starts("##[section]Starting: Initialize job"));
        assert!(starts("##[section]Starting: CmdLine"));
        assert!(!starts("##[section]Finishing: CmdLine"));
        assert!(!starts("##[section]Async Command Start: Foo"));
        assert!(!starts("Starting: CmdLine"));
        assert!(!starts("##[group]Starting: CmdLine"));
        assert!(!starts("##[error]Starting: CmdLine"));
        assert!(!starts("##[debug]Starting: CmdLine"));
    }

    #[test]
    fn section_marker_in_the_middle_of_a_line_is_output() {
        let line = stamped("echo ##[section]not a marker");
        let parsed = parse(&line);

        assert_eq!(parsed.marker, LineMarker::Plain);
        assert_eq!(parsed.body, b"echo ##[section]not a marker");
    }
}
