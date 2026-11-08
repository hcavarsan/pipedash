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
mod tests;
