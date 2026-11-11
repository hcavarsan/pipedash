pub const UPDATE_MAX_CHARS: usize = 500;
pub const NAME_MAX_CHARS: usize = 200;

const MAX_ENTITY_LEN: usize = 16;

pub fn to_plain(html: &str, max: usize) -> String {
    let decoded = decode_entities(&strip_tags(html));
    let mut out = String::with_capacity(decoded.len().min(max * 4));
    let mut count = 0;
    let mut separator = None;

    for c in decoded.chars() {
        if is_forbidden(c) {
            continue;
        }
        if c.is_whitespace() {
            separator = match (separator, c) {
                (_, '\n') | (Some('\n'), _) => Some('\n'),
                _ => Some(' '),
            };
            continue;
        }
        let separator = separator.take().filter(|_| !out.is_empty());
        let needed = 1 + usize::from(separator.is_some());
        if count + needed > max {
            break;
        }
        out.extend(separator);
        out.push(c);
        count += needed;
    }

    out
}

fn is_forbidden(c: char) -> bool {
    (c.is_control() && c != '\n') || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        if !tail.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            out.push('<');
            rest = tail;
            continue;
        }
        let Some(end) = tail.find('>') else {
            out.push_str(&rest[start..]);
            return out;
        };
        if breaks_line(&tail[..end]) {
            out.push('\n');
        }
        rest = &tail[end + 1..];
    }

    out.push_str(rest);
    out
}

fn breaks_line(tag: &str) -> bool {
    let (closing, body) = match tag.strip_prefix('/') {
        Some(body) => (true, body),
        None => (false, tag),
    };
    let name = body
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or_default();

    name.eq_ignore_ascii_case("br") || (closing && name.eq_ignore_ascii_case("p"))
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp + 1..];
        let decoded = tail
            .bytes()
            .take(MAX_ENTITY_LEN + 1)
            .position(|b| b == b';')
            .and_then(|end| Some((decode_entity(&tail[..end])?, end)));
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }

    out.push_str(rest);
    out
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "nbsp" => Some(' '),
        _ => {
            let number = name.strip_prefix('#')?;
            let (digits, radix) = match number.strip_prefix(['x', 'X']) {
                Some(hex) => (hex, 16),
                None => (number, 10),
            };
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return None;
            }
            Some(
                u32::from_str_radix(digits, radix)
                    .ok()
                    .and_then(char::from_u32)
                    .filter(|&c| c != '\0')
                    .unwrap_or(char::REPLACEMENT_CHARACTER),
            )
        }
    }
}

#[cfg(test)]
mod tests;
