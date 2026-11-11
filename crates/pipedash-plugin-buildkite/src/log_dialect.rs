use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const TIMESTAMP_START: &[u8] = b"\x1b_bk;t=";

const SECTIONS: [(&[u8], LineMarker); 4] = [
    (b"^^^ +++", LineMarker::OpenSection),
    (b"--- ", LineMarker::SectionStart { open: false }),
    (b"~~~ ", LineMarker::SectionStart { open: false }),
    (b"+++ ", LineMarker::SectionStart { open: true }),
];

const MESSAGES: [(&[u8], LineMarker); 3] = [
    ("🚨 Error: ".as_bytes(), LineMarker::Error),
    ("⚠️ Warning: ".as_bytes(), LineMarker::Warning),
    (b"$", LineMarker::Command),
];

pub struct BuildkiteLogDialect;

impl LogDialect for BuildkiteLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let (timestamp, body) = split_timestamp(line);
        let text = skip_sgr(body);
        for (prefix, marker) in SECTIONS {
            if let Some(rest) = text.strip_prefix(prefix) {
                return ParsedLine {
                    timestamp,
                    marker,
                    body: rest.strip_prefix(b" ").unwrap_or(rest),
                };
            }
        }
        let marker = MESSAGES
            .iter()
            .find(|(prefix, marker)| {
                text.strip_prefix(*prefix).is_some_and(|rest| {
                    *marker != LineMarker::Command || skip_sgr(rest).starts_with(b" ")
                })
            })
            .map_or(LineMarker::Plain, |(_, marker)| *marker);
        ParsedLine {
            timestamp,
            marker,
            body,
        }
    }

    fn infers_errors(&self) -> bool {
        false
    }
}

fn split_timestamp(line: &[u8]) -> (Option<DateTime<Utc>>, &[u8]) {
    let Some(rest) = line.strip_prefix(TIMESTAMP_START) else {
        return (None, line);
    };
    let Some(end) = rest.iter().position(|b| *b == b'\x07') else {
        return (None, line);
    };
    std::str::from_utf8(&rest[..end])
        .ok()
        .and_then(|ms| ms.parse::<i64>().ok())
        .and_then(DateTime::from_timestamp_millis)
        .map_or((None, line), |ts| (Some(ts), &rest[end + 1..]))
}

fn skip_sgr(mut text: &[u8]) -> &[u8] {
    while let Some(rest) = text.strip_prefix(b"\x1b[") {
        let Some(end) = rest.iter().position(|b| *b == b'm') else {
            break;
        };
        if !rest[..end].iter().all(|b| b.is_ascii_digit() || *b == b';') {
            break;
        }
        text = &rest[end + 1..];
    }
    text
}

#[cfg(test)]
mod tests;
