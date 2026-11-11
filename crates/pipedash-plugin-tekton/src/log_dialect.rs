use std::borrow::Cow;

use chrono::{
    DateTime,
    SecondsFormat,
    Utc,
};
use pipedash_plugin_api::utils::parse_timestamp;
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const STEP_HEADER_TAG: u8 = 0x1e;
const TRUNCATION_TAG: u8 = 0x1f;
const MAX_TIMESTAMP_BYTES: usize = 40;

pub struct TektonLogDialect;

impl LogDialect for TektonLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let (marker, line) = match line.split_first() {
            Some((&STEP_HEADER_TAG, rest)) => (LineMarker::Notice, rest),
            Some((&TRUNCATION_TAG, rest)) => (LineMarker::Warning, rest),
            _ => (LineMarker::Plain, line),
        };
        let (timestamp, body) = split_timestamp(line);
        ParsedLine {
            timestamp,
            marker,
            body,
        }
    }

    fn is_step_start(&self, line: &ParsedLine<'_>) -> bool {
        line.marker == LineMarker::Notice
    }
}

fn split_timestamp(line: &[u8]) -> (Option<DateTime<Utc>>, &[u8]) {
    let head = &line[..line.len().min(MAX_TIMESTAMP_BYTES)];
    let end = head.iter().position(|b| *b == b' ').unwrap_or(head.len());
    std::str::from_utf8(&line[..end])
        .ok()
        .and_then(parse_timestamp)
        .map_or((None, line), |stamp| {
            (Some(stamp), line.get(end + 1..).unwrap_or_default())
        })
}

pub(crate) fn step_header(name: &str, started_at: Option<DateTime<Utc>>) -> Vec<u8> {
    tagged_line(STEP_HEADER_TAG, started_at, &format!("Step {name}"))
}

pub(crate) fn sidecar_header(name: &str, started_at: Option<DateTime<Utc>>) -> Vec<u8> {
    tagged_line(STEP_HEADER_TAG, started_at, &format!("Sidecar {name}"))
}

pub(crate) fn truncation_notice(lines: usize, sidecar: bool) -> Vec<u8> {
    let kind = if sidecar { "sidecar" } else { "step" };
    tagged_line(
        TRUNCATION_TAG,
        None,
        &format!("Pipedash reads at most the last {lines} lines of a {kind}, so older lines may be missing"),
    )
}

fn tagged_line(tag: u8, at: Option<DateTime<Utc>>, text: &str) -> Vec<u8> {
    let mut out = vec![tag];
    if let Some(at) = at {
        out.extend_from_slice(at.to_rfc3339_opts(SecondsFormat::AutoSi, true).as_bytes());
        out.push(b' ');
    }
    out.extend(text.bytes().filter(|b| !b.is_ascii_control()));
    out.push(b'\n');
    out
}

pub(crate) fn without_line_tags(chunk: &[u8]) -> Cow<'_, [u8]> {
    let is_tag = |b: &u8| matches!(*b, STEP_HEADER_TAG | TRUNCATION_TAG);
    if chunk.iter().any(is_tag) {
        Cow::Owned(chunk.iter().copied().filter(|b| !is_tag(b)).collect())
    } else {
        Cow::Borrowed(chunk)
    }
}

#[cfg(test)]
mod tests;
