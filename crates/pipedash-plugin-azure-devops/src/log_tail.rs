use pipedash_plugin_api::{
    content_hash,
    extend_content_hash,
    CONTENT_HASH_SEED,
};
use serde::{
    Deserialize,
    Serialize,
};

fn leading_lines(head: &[u8]) -> Vec<(u64, usize)> {
    let mut lines = Vec::with_capacity(2);
    let mut start = 0;
    while lines.len() < 2 && start < head.len() {
        let (end, next) = match head[start..].iter().position(|&byte| byte == b'\n') {
            Some(offset) => (start + offset, start + offset + 1),
            None => (head.len(), head.len()),
        };
        lines.push((content_hash(&head[start..end]), next));
        start = next;
    }
    lines
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LogPosition {
    pub build: i64,
    pub log: i64,
    pub attempt: i64,
    pub lines: u64,
    pub open: bool,
    pub tail: [u64; 2],
}

impl LogPosition {
    pub fn is_for(&self, build: i64, log: i64, attempt: i64) -> bool {
        self.build == build && self.log == log && self.attempt == attempt
    }

    pub fn request_line(&self) -> u64 {
        self.lines.saturating_sub(1)
    }

    pub fn overlap(&self, head: &[u8]) -> Option<usize> {
        let lines = leading_lines(head);
        let widths: &[usize] = if self.lines == 0 { &[0] } else { &[2, 1] };
        widths
            .iter()
            .copied()
            .find(|&width| {
                width as u64 <= self.lines
                    && lines.len() >= width
                    && lines
                        .iter()
                        .zip(&self.tail[2 - width..])
                        .all(|(line, hash)| line.0 == *hash)
            })
            .map(|width| width.checked_sub(1).map_or(0, |last| lines[last].1))
    }
}

#[derive(Default)]
pub(crate) struct Tally {
    done: u64,
    tail: [u64; 2],
    current: Option<u64>,
    fed: bool,
}

impl Tally {
    pub fn resume(position: &LogPosition) -> Self {
        Self {
            done: position.lines,
            tail: position.tail,
            current: None,
            fed: false,
        }
    }

    pub fn fed(&self) -> bool {
        self.fed
    }

    pub fn feed(&mut self, mut bytes: &[u8]) {
        self.fed |= !bytes.is_empty();
        while !bytes.is_empty() {
            match bytes.iter().position(|&byte| byte == b'\n') {
                Some(end) => {
                    let seed = self.current.take().unwrap_or(CONTENT_HASH_SEED);
                    let hash = extend_content_hash(seed, &bytes[..end]);
                    self.tail = [self.tail[1], hash];
                    self.done += 1;
                    bytes = &bytes[end + 1..];
                }
                None => {
                    let seed = self.current.unwrap_or(CONTENT_HASH_SEED);
                    self.current = Some(extend_content_hash(seed, bytes));
                    return;
                }
            }
        }
    }

    pub fn position(&self, build: i64, log: i64, attempt: i64) -> LogPosition {
        let (lines, tail, open) = match self.current {
            Some(hash) => (self.done + 1, [self.tail[1], hash], true),
            None => (self.done, self.tail, false),
        };
        LogPosition {
            build,
            log,
            attempt,
            lines,
            open,
            tail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &[u8] = b"first\nsecond\n\nfourth\nfifth";

    fn position_after(chunks: &[&[u8]]) -> LogPosition {
        let mut tally = Tally::default();
        for chunk in chunks {
            tally.feed(chunk);
        }
        tally.position(1, 2, 3)
    }

    #[test]
    fn position_does_not_depend_on_how_the_bytes_were_chunked() {
        let whole = position_after(&[LOG]);

        assert_eq!(whole.lines, 5);
        assert!(whole.open);
        for split in 0..=LOG.len() {
            assert_eq!(
                position_after(&[&LOG[..split], &LOG[split..]]),
                whole,
                "{split}"
            );
        }
        let bytewise: Vec<&[u8]> = LOG.chunks(1).collect();
        assert_eq!(position_after(&bytewise), whole);
    }

    #[test]
    fn a_terminated_log_is_not_open_and_an_empty_one_has_no_lines() {
        let terminated = position_after(&[b"a\nb\n"]);
        let empty = position_after(&[]);

        assert_eq!((terminated.lines, terminated.open), (2, false));
        assert_eq!((empty.lines, empty.open), (0, false));
    }

    #[test]
    fn resumed_tally_continues_the_line_count_and_boundary_hashes() {
        let prefix = position_after(&[b"a\nb\n"]);
        let mut resumed = Tally::resume(&prefix);
        resumed.feed(b"c\nd\n");

        assert_eq!(
            resumed.position(1, 2, 3),
            position_after(&[b"a\nb\nc\nd\n"])
        );
    }

    #[test]
    fn overlap_finds_the_boundary_whichever_line_the_server_starts_from() {
        let position = position_after(&[b"a\nb\nc\n"]);

        assert_eq!(position.overlap(b"c\nd\n"), Some(2));
        assert_eq!(position.overlap(b"b\nc\nd\n"), Some(4));
        assert_eq!(position.overlap(b"c\n"), Some(2));
        assert_eq!(position.overlap(b"c"), Some(1));
    }

    #[test]
    fn overlap_rejects_a_slice_that_does_not_continue_the_cursor() {
        let position = position_after(&[b"a\nb\nc\n"]);

        for head in [&b""[..], b"d\ne\n", b"a\nb\n", b"x\nc\n", b"\n"] {
            assert_eq!(position.overlap(head), None, "{head:?}");
        }
    }

    #[test]
    fn empty_cursor_accepts_any_slice_without_skipping() {
        let position = position_after(&[]);

        assert_eq!(position.overlap(b"a\nb\n"), Some(0));
        assert_eq!(position.overlap(b""), Some(0));
    }
}
