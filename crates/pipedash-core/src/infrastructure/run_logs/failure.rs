use std::sync::LazyLock;

use memchr::memmem;

const HEAD_BYTES: usize = 160;
const STAMP_TOKENS: usize = 4;

const UPPER_LEVELS: [&[u8]; 9] = [
    b"ERROR",
    b"ERRO",
    b"FATAL",
    b"CRITICAL",
    b"PANIC",
    b"SEVERE",
    b"FAIL",
    b"FAILED",
    b"FAILURE",
];
const LOWER_LEVELS: [&[u8]; 6] = [b"error", b"Error", b"fatal", b"Fatal", b"panic", b"Panic"];
const STRUCTURED_LEVELS: [&[u8]; 5] = [b"error", b"fatal", b"panic", b"critical", b"eror"];
const PREFIXES: [&[u8]; 10] = [
    b"npm ERR!",
    b"npm error",
    b"Traceback (most recent call last)",
    b"Exception in thread",
    b"Segmentation fault",
    b"make: ***",
    b"--- FAIL",
    b"test result: FAILED",
    b"BUILD FAIL",
    "❌".as_bytes(),
];
const EXIT_PHRASES: [&[u8]; 5] = [
    b"exit code ",
    b"exit status ",
    b"exited with code ",
    b"exited with status ",
    b"non-zero code: ",
];

static LEVEL_KEY: LazyLock<memmem::Finder<'static>> =
    LazyLock::new(|| memmem::Finder::new(b"level"));

pub(super) fn is_error(text: &str) -> bool {
    let bytes = text.as_bytes();
    let head = &bytes[..bytes.len().min(HEAD_BYTES)];
    leading_error(skip_stamps(head)) || structured_level(head) || failed_tail(bytes)
}

fn skip_stamps(mut rest: &[u8]) -> &[u8] {
    for _ in 0..STAMP_TOKENS {
        let text = rest.trim_ascii_start();
        let text = text.strip_prefix(b"[").unwrap_or(text);
        let end = text
            .iter()
            .position(|b| matches!(b, b' ' | b']'))
            .unwrap_or(text.len());
        let token = &text[..end];
        let is_stamp = token.first().is_some_and(u8::is_ascii_digit)
            && token.iter().any(|b| matches!(b, b'-' | b':' | b'/' | b'.'));
        if !is_stamp {
            break;
        }
        rest = text[end..].strip_prefix(b"]").unwrap_or(&text[end..]);
    }
    let rest = rest.trim_ascii_start();
    rest.strip_prefix(b"[").unwrap_or(rest)
}

fn leading_error(text: &[u8]) -> bool {
    if PREFIXES.iter().any(|prefix| text.starts_with(prefix)) {
        return true;
    }
    let name_len = text
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$'))
        .count();
    let name = &text[..name_len];
    let next = text.get(name_len).copied();
    if UPPER_LEVELS.contains(&name) {
        return matches!(next, None | Some(b':' | b' ' | b'\t' | b']' | b'!' | b'['));
    }
    if LOWER_LEVELS.contains(&name) {
        return matches!(next, Some(b':' | b'['));
    }
    next == Some(b':')
        && name.len() > 5
        && (name.ends_with(b"Error") || name.ends_with(b"Exception"))
}

fn structured_level(head: &[u8]) -> bool {
    LEVEL_KEY.find_iter(head).any(|at| {
        let rest = &head[at + 5..];
        let value = if at > 0 && head[at - 1] == b'"' {
            rest.strip_prefix(b"\":")
        } else if at == 0 || !(head[at - 1].is_ascii_alphanumeric() || head[at - 1] == b'_') {
            rest.strip_prefix(b"=")
        } else {
            None
        };
        let Some(value) = value else {
            return false;
        };
        let value = value.trim_ascii_start();
        let value = value.strip_prefix(b"\"").unwrap_or(value);
        STRUCTURED_LEVELS.iter().any(|level| {
            value.len() >= level.len()
                && value[..level.len()].eq_ignore_ascii_case(level)
                && !value
                    .get(level.len())
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
    })
}

fn failed_tail(line: &[u8]) -> bool {
    let line = line.trim_ascii_end();
    if line.ends_with(b" ... FAILED")
        || (line.starts_with(b"exitcode '") && line.ends_with(b"': failure"))
    {
        return true;
    }
    let line = line.strip_suffix(b".").unwrap_or(line);
    let line = line.strip_suffix(b"'").unwrap_or(line);
    let digits = line.iter().rev().take_while(|b| b.is_ascii_digit()).count();
    let (head, code) = line.split_at(line.len() - digits);
    let head = head.strip_suffix(b"'").unwrap_or(head);
    code.iter().any(|b| *b != b'0') && EXIT_PHRASES.iter().any(|phrase| head.ends_with(phrase))
}

#[cfg(test)]
mod tests;
