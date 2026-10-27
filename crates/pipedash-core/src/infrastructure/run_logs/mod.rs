mod ansi;
mod block;
mod document;
mod ingest;
mod outline;
#[cfg(test)]
mod perf;
mod store;
#[cfg(test)]
mod testing;

use std::time::Duration;

pub use document::{
    search_regex,
    BlockCache,
    DocError,
    Document,
};
pub use ingest::{
    BlockLimits,
    IngestOutcome,
    Ingester,
};
pub use outline::StepBound;
pub use store::{
    CacheEpoch,
    DocKey,
    LogStore,
    PublishOutcome,
    ReadLease,
    StoredDocMeta,
    StoredTree,
};

pub const BLOCK_MAX_LINES: usize = 1024;
pub const BLOCK_TARGET_BYTES: usize = 256 * 1024;
pub const BLOCK_MAX_RAW_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
pub const MAX_SPANS_PER_LINE: usize = 4096;
pub const MAX_DOC_TEXT_BYTES: u64 = 1024 * 1024 * 1024;
pub const MARKER_LINES_CAP: usize = 10_000;
pub const MAX_SECTIONS: usize = 20_000;
pub const PAGE_MAX_LINES: u32 = 2000;
pub const PAGE_MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
pub const LINE_UNITS_MIN: u32 = 256;
pub const LINE_UNITS_MAX: u32 = 1_048_576;
pub const SEARCH_MATCH_CAP: usize = 10_000;
pub const SEARCH_MAX_QUERY_CHARS: usize = 1000;
pub const SEARCH_REGEX_SIZE_LIMIT: usize = 10 * 1024 * 1024;
pub const CACHE_BUDGET_BYTES: u64 = if cfg!(target_os = "ios") {
    512 * 1024 * 1024
} else {
    2 * 1024 * 1024 * 1024
};
pub const CACHE_MAX_IDLE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
pub const BLOCK_CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
pub const DOC_CACHE_CAPACITY: usize = 8;
pub const TOUCH_INTERVAL: Duration = Duration::from_secs(60 * 60);
