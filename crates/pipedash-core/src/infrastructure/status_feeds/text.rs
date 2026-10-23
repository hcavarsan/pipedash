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
mod tests {
    use super::*;

    #[test]
    fn br_and_closing_p_become_newlines() {
        assert_eq!(
            to_plain("one<br />two<BR>three <p>four</p>five", 500),
            "one\ntwo\nthree four\nfive"
        );
    }

    #[test]
    fn tags_are_stripped_before_entities_are_decoded() {
        assert_eq!(to_plain("&lt;b&gt;bold&lt;/b&gt;", 500), "<b>bold</b>");
        assert_eq!(to_plain("<b>x</b>&amp;lt;", 500), "x&lt;");
        assert_eq!(to_plain("a &lt;br&gt; b", 500), "a <br> b");
    }

    #[test]
    fn named_and_numeric_entities_decode() {
        assert_eq!(
            to_plain(
                "Tom &amp; Jerry &quot;x&quot; &#39;y&#39; &#x41;&#66; a&nbsp;b &gt; &lt;",
                500
            ),
            "Tom & Jerry \"x\" 'y' AB a b > <"
        );
    }

    #[test]
    fn invalid_numeric_entities_become_replacement_character() {
        assert_eq!(to_plain("a&#xD800;b", 500), "a\u{FFFD}b");
        assert_eq!(to_plain("a&#1114112;b", 500), "a\u{FFFD}b");
        assert_eq!(to_plain("a&#0;b", 500), "a\u{FFFD}b");
        assert_eq!(to_plain("a&#99999999999;b", 500), "a\u{FFFD}b");
    }

    #[test]
    fn unknown_entities_and_stray_markers_stay_literal() {
        assert_eq!(
            to_plain("a &foo; b & c &#; &#x; 1 < 2", 500),
            "a &foo; b & c &#; &#x; 1 < 2"
        );
        assert_eq!(to_plain("broken <a href", 500), "broken <a href");
    }

    #[test]
    fn control_and_bidi_characters_are_removed() {
        assert_eq!(
            to_plain("a\u{0}b\u{7}c\u{85}d\u{9F}e\u{7F}f", 500),
            "abcdef"
        );
        assert_eq!(
            to_plain("a\u{202E}b\u{2066}c\u{2069}d\u{202A}e", 500),
            "abcde"
        );
        assert_eq!(to_plain("x&#7;y&#x202E;z", 500), "xyz");
    }

    #[test]
    fn whitespace_collapses_and_newlines_survive() {
        assert_eq!(to_plain("  a \t\r  b  \n\n  \n c  ", 500), "a b\nc");
    }

    #[test]
    fn output_is_capped_by_characters() {
        assert_eq!(to_plain("ééééé", 3), "ééé");
        assert_eq!(to_plain("ab cd", 3), "ab");
        assert_eq!(
            to_plain(&"x".repeat(900), UPDATE_MAX_CHARS).chars().count(),
            500
        );
        assert_eq!(
            to_plain(&"y".repeat(900), NAME_MAX_CHARS).chars().count(),
            200
        );
    }

    #[test]
    fn pathological_input_stays_linear() {
        let text = "<a".repeat(200_000) + &"&".repeat(200_000);

        assert_eq!(to_plain(&text, 500).chars().count(), 500);
    }
}
