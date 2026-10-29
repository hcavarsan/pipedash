use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const PIPELINE_PREFIX: &[u8] = b"[Pipeline] ";
const BODY_START: &[u8] = b"{ (";
const STAGE_END: &[u8] = b"// stage";
const SHELL_STEP: &[u8] = b"sh";
const PARALLEL_BRANCH: &[u8] = b"Branch: ";
const ERROR_PREFIX: &[u8] = b"ERROR: ";
const FAILED_BUILDS: [&[u8]; 2] = [b"Finished: FAILURE", b"Finished: UNSTABLE"];
const TRACED_COMMAND: &[u8] = b"+ ";

pub struct JenkinsLogDialect;

impl LogDialect for JenkinsLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let (marker, body) = match line.strip_prefix(PIPELINE_PREFIX) {
            Some(step) => classify_step(line, step),
            None => (classify_output(line), line),
        };
        ParsedLine {
            timestamp: None,
            marker,
            body,
        }
    }

    fn infers_errors(&self) -> bool {
        false
    }
}

fn classify_step<'a>(line: &'a [u8], step: &'a [u8]) -> (LineMarker, &'a [u8]) {
    if step == STAGE_END {
        (LineMarker::GroupEnd, &[])
    } else if step == SHELL_STEP {
        (LineMarker::Command, line)
    } else if let Some(name) = stage_name(step) {
        (LineMarker::SectionStart { open: false }, name)
    } else {
        (LineMarker::Plain, line)
    }
}

fn stage_name(step: &[u8]) -> Option<&[u8]> {
    step.strip_prefix(BODY_START)?
        .strip_suffix(b")")
        .filter(|name| !name.is_empty() && !name.starts_with(PARALLEL_BRANCH))
}

fn classify_output(line: &[u8]) -> LineMarker {
    if line.starts_with(ERROR_PREFIX) || FAILED_BUILDS.contains(&line) {
        LineMarker::Error
    } else if line.starts_with(TRACED_COMMAND) {
        LineMarker::Command
    } else {
        LineMarker::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> ParsedLine<'_> {
        JenkinsLogDialect.parse(line)
    }

    #[test]
    fn stage_body_opening_starts_a_section_named_after_the_stage() {
        let cases: [(&[u8], &[u8]); 4] = [
            (b"[Pipeline] { (Build)", b"Build"),
            (
                b"[Pipeline] { (Declarative: Checkout SCM)",
                b"Declarative: Checkout SCM",
            ),
            (b"[Pipeline] { (Deploy (prod))", b"Deploy (prod)"),
            (b"[Pipeline] { (build & test)", b"build & test"),
        ];
        for (line, name) in cases {
            let parsed = parse(line);
            assert_eq!(
                parsed.marker,
                LineMarker::SectionStart { open: false },
                "{line:?}"
            );
            assert_eq!(parsed.body, name, "{line:?}");
        }
    }

    #[test]
    fn stage_step_close_ends_the_group() {
        let parsed = parse(b"[Pipeline] // stage");
        assert_eq!(parsed.marker, LineMarker::GroupEnd);
    }

    #[test]
    fn other_pipeline_lines_stay_plain() {
        let lines: [&[u8]; 12] = [
            b"[Pipeline] stage",
            b"[Pipeline] {",
            b"[Pipeline] }",
            b"[Pipeline] // node",
            b"[Pipeline] // stages",
            b"[Pipeline] echo",
            b"[Pipeline] node",
            b"[Pipeline] Start of Pipeline",
            b"[Pipeline] { (Branch: unit)",
            b"[Pipeline] { ()",
            b"[Pipeline] { (unclosed",
            b"[Pipeline] shell",
        ];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
        }
    }

    #[test]
    fn shell_step_and_traced_commands_are_commands() {
        let lines: [&[u8]; 3] = [b"[Pipeline] sh", b"+ make test", b"+ echo \"a + b\""];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Command, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
        }
    }

    #[test]
    fn plus_without_a_space_is_output() {
        for line in [b"+make".as_slice(), b"++ nested", b"a + b", b"+"] {
            assert_eq!(parse(line).marker, LineMarker::Plain, "{line:?}");
        }
    }

    #[test]
    fn error_lines_and_failed_results_are_errors() {
        let lines: [&[u8]; 4] = [
            b"ERROR: script returned exit code 1",
            b"ERROR: Build step failed with exception",
            b"Finished: FAILURE",
            b"Finished: UNSTABLE",
        ];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Error, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
        }
    }

    #[test]
    fn other_results_and_look_alikes_are_output() {
        let lines: [&[u8]; 5] = [
            b"Finished: SUCCESS",
            b"Finished: ABORTED",
            b"Finished: FAILURE twice",
            b"ERROR:",
            b"error: lowercase",
        ];
        for line in lines {
            assert_eq!(parse(line).marker, LineMarker::Plain, "{line:?}");
        }
    }

    #[test]
    fn freestyle_console_has_no_sections() {
        let lines: [&[u8]; 6] = [
            b"Started by user alice",
            b"Running as SYSTEM",
            b"Building in workspace /var/jenkins_home/workspace/app",
            b"[app] $ /bin/sh -xe /tmp/jenkins123.sh",
            b"Hello from the build",
            b"Finished: SUCCESS",
        ];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.body, line, "{line:?}");
            assert_eq!(parsed.timestamp, None);
        }
    }
}
