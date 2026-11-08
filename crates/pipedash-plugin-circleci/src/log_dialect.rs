use chrono::{
    DateTime,
    SecondsFormat,
    Utc,
};
use pipedash_plugin_api::logs::tagged::{
    tag_of,
    TaggedLogDialect,
};
use pipedash_plugin_api::LineMarker;

use crate::types::OutputMessage;

const NO_TIMESTAMP: &str = "-";

const TAGS: &[(u8, LineMarker)] = &[
    (b'.', LineMarker::Plain),
    (b'#', LineMarker::SectionStart { open: false }),
    (b'+', LineMarker::SectionStart { open: true }),
];

pub(crate) const DIALECT: TaggedLogDialect = TaggedLogDialect {
    tags: TAGS,
    infer_errors: true,
};

pub(crate) struct Section<'a> {
    pub name: String,
    pub failed: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub output: Option<&'a [OutputMessage]>,
}

pub(crate) const OMITTED: &str =
    "This step printed more output than Pipedash loads. Open the job in CircleCI to see it.";

pub(crate) fn render(sections: &[Section<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for section in sections {
        let marker = LineMarker::SectionStart {
            open: section.failed,
        };
        push_line(&mut out, marker, section.started_at, &section.name);
        let Some(output) = section.output else {
            push_line(&mut out, LineMarker::Plain, None, OMITTED);
            continue;
        };
        let mut line = String::new();
        let mut line_at = None;
        for message in output {
            for piece in message.message.split_inclusive('\n') {
                if line.is_empty() {
                    line_at = message.time;
                }
                line.push_str(piece);
                if line.ends_with('\n') {
                    push_line(&mut out, LineMarker::Plain, line_at, &line);
                    line.clear();
                }
            }
        }
        if !line.is_empty() {
            push_line(&mut out, LineMarker::Plain, line_at, &line);
        }
    }
    out
}

fn push_line(out: &mut Vec<u8>, marker: LineMarker, at: Option<DateTime<Utc>>, text: &str) {
    let tag = tag_of(TAGS, marker);
    let stamp = at.map(|at| at.to_rfc3339_opts(SecondsFormat::AutoSi, true));
    let stamp = stamp.as_deref().unwrap_or(NO_TIMESTAMP);
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    for (index, segment) in text.split('\r').enumerate() {
        if index > 0 {
            out.push(b'\r');
        }
        out.push(tag);
        out.extend_from_slice(stamp.as_bytes());
        out.push(b' ');
        out.extend_from_slice(segment.as_bytes());
    }
    out.push(b'\n');
}

#[cfg(test)]
mod tests;
