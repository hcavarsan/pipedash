use chrono::{
    DateTime,
    SecondsFormat,
    Utc,
};

use super::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const NO_TIMESTAMP: &str = "-";

pub const TAGGED_MARKERS: &[(u8, LineMarker)] = &[
    (b'.', LineMarker::Plain),
    (b'!', LineMarker::Error),
    (b'?', LineMarker::Warning),
    (b'#', LineMarker::SectionStart { open: false }),
    (b'+', LineMarker::SectionStart { open: true }),
];

pub struct TaggedLogDialect {
    pub tags: &'static [(u8, LineMarker)],
    pub infer_errors: bool,
}

fn split_tag<'a>(tags: &[(u8, LineMarker)], line: &'a [u8]) -> Option<(LineMarker, &'a [u8])> {
    let (tag, rest) = line.split_first()?;
    let (_, marker) = tags.iter().find(|(known, _)| known == tag)?;
    Some((*marker, rest))
}

pub fn tag_of(tags: &[(u8, LineMarker)], marker: LineMarker) -> u8 {
    tags.iter()
        .find(|(_, known)| *known == marker)
        .map_or(b'.', |(tag, _)| *tag)
}

impl LogDialect for TaggedLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let Some((marker, rest)) = split_tag(self.tags, line) else {
            return ParsedLine::plain(line);
        };
        let Some(space) = rest.iter().position(|b| *b == b' ') else {
            return ParsedLine::plain(line);
        };
        let timestamp = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(crate::utils::parse_timestamp);
        ParsedLine {
            timestamp,
            marker,
            body: &rest[space + 1..],
        }
    }

    fn infers_errors(&self) -> bool {
        self.infer_errors
    }
}

pub struct SpacedTagDialect {
    pub tags: &'static [(u8, LineMarker)],
}

impl LogDialect for SpacedTagDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let Some((marker, text)) = split_tag(self.tags, line)
            .and_then(|(marker, rest)| Some((marker, rest.strip_prefix(b" ")?)))
        else {
            return ParsedLine::plain(line);
        };
        match marker {
            LineMarker::Command => ParsedLine {
                marker,
                ..ParsedLine::plain(line)
            },
            _ => ParsedLine {
                timestamp: None,
                marker,
                body: text,
            },
        }
    }

    fn infers_errors(&self) -> bool {
        false
    }
}

pub fn write_spaced_line(
    out: &mut Vec<u8>, tags: &[(u8, LineMarker)], marker: LineMarker, line: &str,
) {
    let tag = tag_of(tags, marker);
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

pub fn write_line(out: &mut Vec<u8>, marker: LineMarker, stamp: Option<&str>, text: &str) {
    let tag = tag_of(TAGGED_MARKERS, marker);
    for line in text.split("\r\n").flat_map(|line| line.split(['\n', '\r'])) {
        out.push(tag);
        out.extend_from_slice(stamp.unwrap_or(NO_TIMESTAMP).as_bytes());
        out.push(b' ');
        out.extend_from_slice(line.as_bytes());
        out.push(b'\n');
    }
}

pub fn write_line_at(out: &mut Vec<u8>, marker: LineMarker, at: Option<DateTime<Utc>>, text: &str) {
    let stamp = at.map(|at| at.to_rfc3339_opts(SecondsFormat::AutoSi, true));
    write_line(out, marker, stamp.as_deref(), text);
}

pub trait LogEvent {
    fn is_warning(&self) -> bool;

    fn event_time(&self) -> Option<DateTime<Utc>>;

    fn event_text(&self) -> String;
}

pub fn write_events<E: LogEvent>(out: &mut Vec<u8>, events: &[E], empty: &str) {
    if events.is_empty() {
        write_line(out, LineMarker::Plain, None, empty);
    }
    let mut events: Vec<_> = events.iter().collect();
    events.sort_by_key(|event| event.event_time());
    for event in events {
        let marker = if event.is_warning() {
            LineMarker::Warning
        } else {
            LineMarker::Plain
        };
        write_line_at(out, marker, event.event_time(), &event.event_text());
    }
}

#[cfg(test)]
mod tests;
