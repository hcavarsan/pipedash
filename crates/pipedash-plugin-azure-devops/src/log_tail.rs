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
mod tests;
