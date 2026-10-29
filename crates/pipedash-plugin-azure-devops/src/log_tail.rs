use bytes::Bytes;
use pipedash_plugin_api::{
    extend_content_hash,
    CONTENT_HASH_SEED,
};
use serde::{
    Deserialize,
    Serialize,
};

pub(crate) const BOUNDARY_LINE_LIMIT: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Anchor {
    line: u64,
    before: u64,
    after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LogPosition {
    pub build: i64,
    pub log: i64,
    pub attempt: i64,
    pub lines: u64,
    pub open: bool,
    pub last: u64,
    pub anchor: Option<Anchor>,
    pub first_line: Option<u64>,
}

impl LogPosition {
    pub fn is_for(&self, build: i64, log: i64, attempt: i64) -> bool {
        self.build == build && self.log == log && self.attempt == attempt
    }

    pub fn probe_line(&self) -> Option<u64> {
        self.anchor.map(|anchor| anchor.line + 1)
    }

    pub fn settle(&mut self, first: u64) -> bool {
        let Some(anchor) = self.anchor else {
            return false;
        };
        self.first_line = if first == anchor.after {
            Some(0)
        } else if first == anchor.before {
            Some(1)
        } else {
            return false;
        };
        self.anchor = None;
        true
    }

    pub fn boundary_line(&self) -> Option<u64> {
        self.first_line
            .map(|first| self.lines.saturating_sub(1) + first)
    }
}

pub(crate) enum Scan {
    More,
    Line(u64, Bytes),
    TooLong,
}

pub(crate) struct BoundaryScan {
    hash: u64,
    scanned: usize,
}

impl BoundaryScan {
    pub fn new() -> Self {
        Self {
            hash: CONTENT_HASH_SEED,
            scanned: 0,
        }
    }

    pub fn feed(&mut self, chunk: Bytes) -> Scan {
        let window = chunk.len().min(BOUNDARY_LINE_LIMIT - self.scanned + 1);
        match chunk[..window].iter().position(|&byte| byte == b'\n') {
            Some(end) => Scan::Line(
                extend_content_hash(self.hash, &chunk[..end]),
                chunk.slice(end + 1..),
            ),
            None => {
                self.hash = extend_content_hash(self.hash, &chunk[..window]);
                self.scanned += window;
                if self.scanned > BOUNDARY_LINE_LIMIT {
                    Scan::TooLong
                } else {
                    Scan::More
                }
            }
        }
    }

    pub fn finish(self) -> Option<u64> {
        (self.scanned > 0).then_some(self.hash)
    }
}

#[derive(Default)]
pub(crate) struct Tally {
    done: u64,
    last: u64,
    current: Option<u64>,
    anchor: Option<Anchor>,
    first_line: Option<u64>,
    fed: bool,
}

impl Tally {
    pub fn resume(position: &LogPosition) -> Self {
        Self {
            done: position.lines,
            last: position.last,
            current: None,
            anchor: None,
            first_line: position.first_line,
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
                    if self.first_line.is_none() && self.done > 0 && hash != self.last {
                        self.anchor = Some(Anchor {
                            line: self.done - 1,
                            before: self.last,
                            after: hash,
                        });
                    }
                    self.last = hash;
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
        let (lines, last, open) = match self.current {
            Some(hash) => (self.done + 1, hash, true),
            None => (self.done, self.last, false),
        };
        LogPosition {
            build,
            log,
            attempt,
            lines,
            open,
            last,
            anchor: self.anchor.filter(|_| self.first_line.is_none()),
            first_line: self.first_line,
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

    fn first_line_hash(bytes: &'static [u8]) -> Option<u64> {
        let mut scan = BoundaryScan::new();
        match scan.feed(Bytes::from_static(bytes)) {
            Scan::Line(hash, _) => Some(hash),
            _ => scan.finish(),
        }
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
    fn resumed_tally_continues_the_line_count_and_last_line() {
        let mut prefix = position_after(&[b"a\nb\n"]);
        assert!(prefix.settle(first_line_hash(b"b\n").unwrap()));
        let mut resumed = Tally::resume(&prefix);
        resumed.feed(b"c\nd\n");

        let mut whole = position_after(&[b"a\nb\nc\nd\n"]);
        whole.first_line = Some(0);
        whole.anchor = None;
        assert_eq!(resumed.position(1, 2, 3), whole);
    }

    #[test]
    fn the_anchor_is_the_latest_pair_of_neighbouring_lines_that_differ() {
        let position = position_after(&[b"a\nb\nb\nb\n"]);

        assert_eq!(position.probe_line(), Some(1));
        assert!(position_after(&[b"a\n"]).probe_line().is_none());
        assert!(position_after(&[b"z\nz\nz\n"]).probe_line().is_none());
        assert!(position_after(&[]).probe_line().is_none());
    }

    #[test]
    fn probing_a_line_tells_the_first_line_number_of_the_server() {
        let hash_a = first_line_hash(b"a\n").unwrap();
        let hash_b = first_line_hash(b"b\n").unwrap();
        let hash_c = first_line_hash(b"c\n").unwrap();

        for (probed, first_line) in [(hash_b, Some(0)), (hash_a, Some(1)), (hash_c, None)] {
            let mut position = position_after(&[b"a\nb\n"]);

            assert_eq!(position.settle(probed), first_line.is_some());
            assert_eq!(position.first_line, first_line);
            assert_eq!(position.anchor.is_some(), first_line.is_none());
        }
    }

    #[test]
    fn boundary_line_is_the_last_known_line_in_the_servers_numbering() {
        let mut position = position_after(&[b"a\nb\nc\n"]);

        assert_eq!(position.boundary_line(), None);
        position.first_line = Some(0);
        assert_eq!(position.boundary_line(), Some(2));
        position.first_line = Some(1);
        assert_eq!(position.boundary_line(), Some(3));
    }

    #[test]
    fn boundary_scan_hashes_the_first_line_across_chunks_and_returns_the_rest() {
        let mut scan = BoundaryScan::new();

        assert!(matches!(scan.feed(Bytes::from_static(b"he")), Scan::More));
        let Scan::Line(hash, rest) = scan.feed(Bytes::from_static(b"llo\nworld\n")) else {
            panic!("the line ends in this chunk");
        };

        assert_eq!(Some(hash), first_line_hash(b"hello\n"));
        assert_eq!(&rest[..], b"world\n");
        assert_eq!(
            first_line_hash(b"hello"),
            first_line_hash(b"hello\n"),
            "an unterminated line at the end of the body is still a line"
        );
        assert_eq!(first_line_hash(b""), None);
        assert_eq!(
            first_line_hash(b"\n"),
            Some(CONTENT_HASH_SEED),
            "an empty first line is still a line"
        );
    }

    #[test]
    fn boundary_scan_gives_up_on_a_line_longer_than_the_limit_without_keeping_it() {
        let chunk = Bytes::from(vec![b'x'; 64 * 1024]);
        let mut scan = BoundaryScan::new();
        let mut fed = 0;

        let verdict = loop {
            match scan.feed(chunk.clone()) {
                Scan::More => fed += chunk.len(),
                other => break other,
            }
        };

        assert!(matches!(verdict, Scan::TooLong));
        assert!(fed <= BOUNDARY_LINE_LIMIT);
        assert!(scan.scanned <= BOUNDARY_LINE_LIMIT + 1);
    }

    #[test]
    fn boundary_scan_accepts_a_line_of_exactly_the_limit() {
        let mut scan = BoundaryScan::new();
        let mut line = vec![b'x'; BOUNDARY_LINE_LIMIT];
        line.push(b'\n');

        assert!(matches!(
            scan.feed(Bytes::from(line)),
            Scan::Line(_, rest) if rest.is_empty()
        ));
        let mut scan = BoundaryScan::new();
        assert!(matches!(
            scan.feed(Bytes::from(vec![b'x'; BOUNDARY_LINE_LIMIT + 1])),
            Scan::TooLong
        ));
    }
}
