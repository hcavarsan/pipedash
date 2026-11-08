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
mod tests;
