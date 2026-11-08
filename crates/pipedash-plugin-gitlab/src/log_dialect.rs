use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::utils::parse_timestamp;
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const HEADER_LEN: usize = 32;
const DATETIME_LEN: usize = 27;
const SECTION_START: &[u8] = b"section_start:";
const SECTION_END: &[u8] = b"section_end:";
const CLEAR_LINE: &[u8] = b"\r\x1b[0K";

pub struct GitlabLogDialect;

struct Marker<'a> {
    start: bool,
    unix: i64,
    name: &'a [u8],
    collapsed: bool,
    rest: &'a [u8],
}

impl LogDialect for GitlabLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let (header_time, rest) = split_header(line);
        let plain = ParsedLine {
            timestamp: header_time,
            marker: LineMarker::Plain,
            body: rest,
        };

        let mut text = skip_csi(rest);
        let mut started = None;
        let mut ended = None;
        let mut body = rest;
        while let Some(marker) = parse_marker(text) {
            if marker.start {
                started = Some((marker.name, marker.unix, marker.collapsed));
            } else {
                ended = Some(marker.unix);
            }
            body = trim_carriage_returns(marker.rest);
            text = skip_csi(marker.rest);
        }

        if let Some((name, unix, collapsed)) = started {
            return ParsedLine {
                timestamp: header_time.or_else(|| from_unix(unix)),
                marker: LineMarker::SectionStart { open: !collapsed },
                body: if is_blank(body) { name } else { body },
            };
        }
        let Some(unix) = ended else {
            return plain;
        };
        ParsedLine {
            timestamp: header_time.or_else(|| from_unix(unix)),
            marker: LineMarker::GroupEnd,
            body: if is_blank(body) { &[] } else { body },
        }
    }
}

fn split_header(line: &[u8]) -> (Option<DateTime<Utc>>, &[u8]) {
    let Some(header) = line.get(..HEADER_LEN) else {
        return (None, line);
    };
    let well_formed = header[DATETIME_LEN - 1] == b'Z'
        && header[DATETIME_LEN] == b' '
        && header[DATETIME_LEN + 1..DATETIME_LEN + 3]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        && matches!(header[DATETIME_LEN + 3], b'E' | b'O')
        && matches!(header[DATETIME_LEN + 4], b'+' | b' ');
    let time = well_formed
        .then(|| std::str::from_utf8(&header[..DATETIME_LEN]).ok())
        .flatten()
        .and_then(parse_timestamp);
    match time {
        Some(time) => (Some(time), &line[HEADER_LEN..]),
        None => (None, line),
    }
}

fn parse_marker(text: &[u8]) -> Option<Marker<'_>> {
    let (start, rest) = match text.strip_prefix(SECTION_START) {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix(SECTION_END)?),
    };
    let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || rest.get(digits) != Some(&b':') {
        return None;
    }
    let unix = std::str::from_utf8(&rest[..digits]).ok()?.parse().ok()?;
    let rest = &rest[digits + 1..];
    let name_len = rest
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(**b, b'_' | b'.' | b'-'))
        .count();
    if name_len == 0 {
        return None;
    }
    let (name, mut rest) = rest.split_at(name_len);
    let mut collapsed = false;
    if rest.first() == Some(&b'[') {
        let close = rest.iter().position(|b| *b == b']')?;
        collapsed = rest[1..close]
            .split(|b| *b == b',')
            .any(|option| option.trim_ascii() == b"collapsed=true");
        rest = &rest[close + 1..];
    }
    Some(Marker {
        start,
        unix,
        name,
        collapsed,
        rest: rest.strip_prefix(CLEAR_LINE)?,
    })
}

fn from_unix(seconds: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(seconds, 0)
}

fn skip_csi(mut text: &[u8]) -> &[u8] {
    while let Some(rest) = text.strip_prefix(b"\x1b[") {
        let Some(end) = rest.iter().position(|b| (0x40..=0x7e).contains(b)) else {
            break;
        };
        text = &rest[end + 1..];
    }
    text
}

fn is_blank(mut text: &[u8]) -> bool {
    loop {
        text = skip_csi(text);
        match text.split_first() {
            None => return true,
            Some((byte, rest)) if byte.is_ascii_whitespace() => text = rest,
            Some(_) => return false,
        }
    }
}

fn trim_carriage_returns(mut text: &[u8]) -> &[u8] {
    while let Some(rest) = text.strip_suffix(b"\r") {
        text = rest;
    }
    text
}

#[cfg(test)]
mod tests;
