use pipedash_plugin_api::logs::tagged::{
    write_spaced_line,
    SpacedTagDialect,
};
use pipedash_plugin_api::LineMarker;

use crate::types::UpdateLog;

const TAGS: &[(u8, LineMarker)] = &[
    (b'.', LineMarker::Plain),
    (b'$', LineMarker::Command),
    (b'!', LineMarker::Error),
    (b'?', LineMarker::Warning),
];

pub(crate) const DIALECT: SpacedTagDialect = SpacedTagDialect { tags: TAGS };

pub(crate) fn stage_lines(log: &UpdateLog) -> impl Iterator<Item = (LineMarker, &str)> {
    text_lines(&log.command, &log.stdout, &log.stderr, log.success)
}

pub(crate) fn text_lines<'a>(
    command: &'a str, stdout: &'a str, stderr: &'a str, success: bool,
) -> impl Iterator<Item = (LineMarker, &'a str)> {
    let stderr_marker = if success {
        LineMarker::Warning
    } else {
        LineMarker::Error
    };
    [
        (LineMarker::Command, command),
        (LineMarker::Plain, stdout),
        (stderr_marker, stderr),
    ]
    .into_iter()
    .flat_map(|(marker, text)| text.lines().map(move |line| (marker, line)))
}

pub(crate) fn push_line(out: &mut Vec<u8>, marker: LineMarker, line: &str) {
    write_spaced_line(out, TAGS, marker, line);
}

#[cfg(test)]
mod tests;
