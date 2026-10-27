use std::borrow::Cow;
use std::collections::{
    BTreeMap,
    HashMap,
};
use std::fs::{
    self,
    File,
};
use std::io::{
    self,
    Read,
    Seek,
    SeekFrom,
};
use std::ops::Range;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};
use std::thread;

use bytes::Bytes;
use chrono::{
    DateTime,
    SecondsFormat,
};
use regex::{
    Regex,
    RegexBuilder,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::ansi::Span;
use super::block::{
    self,
    BlockRef,
    DecodedBlock,
    LineMeta,
    INDEX_RECORD_BYTES,
};
use super::ingest::{
    BLOCKS_FILE,
    INDEX_FILE,
    META_FILE,
};
use super::store::StoredDocMeta;
use super::{
    BLOCK_CACHE_BUDGET_BYTES,
    BLOCK_MAX_RAW_BYTES,
    LINE_UNITS_MAX,
    LINE_UNITS_MIN,
    PAGE_MAX_LINES,
    PAGE_MAX_TEXT_BYTES,
    SEARCH_MATCH_CAP,
    SEARCH_MAX_QUERY_CHARS,
    SEARCH_REGEX_SIZE_LIMIT,
};
use crate::domain::{
    DomainError,
    DomainResult,
    LogLine,
    LogLinesPage,
    LogSearchQuery,
    LogSearchResult,
};

const MAX_SEARCH_WORKERS: usize = 4;
const COMPRESSED_SLACK_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum DocError {
    #[error("Log cache is corrupted")]
    Corrupt,
    #[error("Log cache I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Search cancelled")]
    Cancelled,
}

impl DocError {
    pub fn is_missing(&self) -> bool {
        matches!(self, DocError::Io(e) if e.kind() == io::ErrorKind::NotFound)
    }
}

pub fn search_regex(query: &LogSearchQuery) -> DomainResult<Option<Regex>> {
    if query.text.is_empty() {
        return Ok(None);
    }
    if query.text.chars().nth(SEARCH_MAX_QUERY_CHARS).is_some() {
        return Err(DomainError::InvalidConfig("Search text is too long".into()));
    }
    let pattern = if query.regex {
        Cow::Borrowed(query.text.as_str())
    } else {
        Cow::Owned(regex::escape(&query.text))
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .size_limit(SEARCH_REGEX_SIZE_LIMIT)
        .build()
        .map(Some)
        .map_err(|e| DomainError::InvalidConfig(format!("Invalid search pattern: {e}")))
}

type CacheKey = (PathBuf, u32);

struct CacheEntry {
    block: Arc<DecodedBlock>,
    size: usize,
    tick: u64,
}

#[derive(Default)]
struct CacheInner {
    map: HashMap<CacheKey, CacheEntry>,
    order: BTreeMap<u64, CacheKey>,
    tick: u64,
    total: usize,
}

pub struct BlockCache {
    inner: Mutex<CacheInner>,
    budget: usize,
}

impl Default for BlockCache {
    fn default() -> Self {
        Self::new(BLOCK_CACHE_BUDGET_BYTES)
    }
}

impl BlockCache {
    pub fn new(budget: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner::default()),
            budget,
        }
    }

    fn get(&self, key: &CacheKey) -> Option<Arc<DecodedBlock>> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.tick += 1;
        let tick = inner.tick;
        let entry = inner.map.get_mut(key)?;
        let old = std::mem::replace(&mut entry.tick, tick);
        let block = entry.block.clone();
        inner.order.remove(&old);
        inner.order.insert(tick, key.clone());
        Some(block)
    }

    fn insert(&self, key: CacheKey, block: Arc<DecodedBlock>) {
        let size = block.text.len()
            + block.lines.len() * size_of::<LineMeta>()
            + block.spans.len() * size_of::<Span>();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.tick += 1;
        let tick = inner.tick;
        if let Some(old) = inner
            .map
            .insert(key.clone(), CacheEntry { block, size, tick })
        {
            inner.total -= old.size;
            inner.order.remove(&old.tick);
        }
        inner.order.insert(tick, key);
        inner.total += size;
        while inner.total > self.budget && inner.order.len() > 1 {
            let Some((_, oldest)) = inner.order.pop_first() else {
                break;
            };
            if let Some(entry) = inner.map.remove(&oldest) {
                inner.total -= entry.size;
            }
        }
    }

    pub fn forget(&self, dir: &Path) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let keys: Vec<CacheKey> = inner.map.keys().filter(|k| k.0 == dir).cloned().collect();
        for key in keys {
            if let Some(entry) = inner.map.remove(&key) {
                inner.total -= entry.size;
                inner.order.remove(&entry.tick);
            }
        }
    }
}

pub struct Document {
    dir: PathBuf,
    meta: Arc<StoredDocMeta>,
    blocks: Arc<[BlockRef]>,
}

struct Partial {
    matches: Vec<(u32, u32, u32)>,
    total: u64,
}

impl Document {
    pub fn open(dir: &Path) -> Result<Self, DocError> {
        let meta: StoredDocMeta = serde_json::from_slice(&fs::read(dir.join(META_FILE))?)
            .map_err(|_| DocError::Corrupt)?;
        if meta.format != 1 {
            return Err(DocError::Corrupt);
        }
        let index = fs::read(dir.join(INDEX_FILE))?;
        let blocks_len = fs::metadata(dir.join(BLOCKS_FILE))?.len();
        let (records, rest) = index.as_chunks::<INDEX_RECORD_BYTES>();
        if !rest.is_empty() {
            return Err(DocError::Corrupt);
        }
        let blocks: Vec<BlockRef> = records
            .iter()
            .filter_map(|record| BlockRef::from_bytes(record))
            .collect();
        if blocks.is_empty() != (meta.line_count == 0) {
            return Err(DocError::Corrupt);
        }
        let mut previous: Option<u32> = None;
        for block in &blocks {
            let ordered = match previous {
                None => block.first_line == 0,
                Some(first) => block.first_line > first,
            };
            let end = block.offset.checked_add(u64::from(block.compressed_len));
            if !ordered
                || block.first_line >= meta.line_count
                || end.is_none_or(|end| end > blocks_len)
                || block.compressed_len as usize > BLOCK_MAX_RAW_BYTES + COMPRESSED_SLACK_BYTES
            {
                return Err(DocError::Corrupt);
            }
            previous = Some(block.first_line);
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            meta: Arc::new(meta),
            blocks: blocks.into(),
        })
    }

    pub fn meta(&self) -> &StoredDocMeta {
        &self.meta
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    fn block_lines(&self, index: usize) -> u32 {
        let next = self
            .blocks
            .get(index + 1)
            .map_or(self.meta.line_count, |b| b.first_line);
        next - self.blocks[index].first_line
    }

    fn load_block(
        &self, file: &mut Option<File>, scratch: &mut Vec<u8>, index: usize,
    ) -> Result<DecodedBlock, DocError> {
        let block_ref = self.blocks[index];
        let file = match file {
            Some(file) => file,
            None => file.insert(File::open(self.dir.join(BLOCKS_FILE))?),
        };
        file.seek(SeekFrom::Start(block_ref.offset))?;
        scratch.resize(block_ref.compressed_len as usize, 0);
        file.read_exact(scratch).map_err(|e| match e.kind() {
            io::ErrorKind::UnexpectedEof => DocError::Corrupt,
            _ => DocError::Io(e),
        })?;
        let decoded =
            block::decode(scratch, self.meta.styles.len()).map_err(|_| DocError::Corrupt)?;
        if decoded.lines.len() != self.block_lines(index) as usize {
            return Err(DocError::Corrupt);
        }
        Ok(decoded)
    }

    fn cached_block(
        &self, cache: &BlockCache, file: &mut Option<File>, scratch: &mut Vec<u8>, index: usize,
    ) -> Result<Arc<DecodedBlock>, DocError> {
        let key = (self.dir.clone(), index as u32);
        if let Some(block) = cache.get(&key) {
            return Ok(block);
        }
        let block = Arc::new(self.load_block(file, scratch, index)?);
        cache.insert(key, block.clone());
        Ok(block)
    }

    pub fn read_lines(
        &self, cache: &BlockCache, start: u32, count: u32, max_units: u32,
    ) -> Result<LogLinesPage, DocError> {
        let count = count.min(PAGE_MAX_LINES);
        let max_units = max_units.clamp(LINE_UNITS_MIN, LINE_UNITS_MAX) as usize;
        let line_count = self.meta.line_count;
        let mut lines = Vec::new();
        if start >= line_count || count == 0 {
            return Ok(LogLinesPage { start, lines });
        }
        let end = start.saturating_add(count).min(line_count);
        let mut index = self
            .blocks
            .partition_point(|b| b.first_line <= start)
            .checked_sub(1)
            .ok_or(DocError::Corrupt)?;
        let mut file = None;
        let mut scratch = Vec::new();
        let mut next = start;
        let mut page_bytes = 0usize;
        'blocks: while next < end && index < self.blocks.len() {
            let block = self.cached_block(cache, &mut file, &mut scratch, index)?;
            let skip = (next - self.blocks[index].first_line) as usize;
            for meta in block.lines.iter().skip(skip) {
                if next >= end {
                    break 'blocks;
                }
                let line = render_line(&block, meta, max_units);
                page_bytes += line.text.len();
                lines.push(line);
                next += 1;
                if page_bytes > PAGE_MAX_TEXT_BYTES {
                    break 'blocks;
                }
            }
            index += 1;
        }
        Ok(LogLinesPage { start, lines })
    }

    pub fn search(
        &self, regex: &Regex, cancel: &CancellationToken,
    ) -> Result<LogSearchResult, DocError> {
        let total_blocks = self.blocks.len();
        if total_blocks == 0 {
            return Ok(LogSearchResult::default());
        }
        let workers = thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(MAX_SEARCH_WORKERS)
            .min(total_blocks);
        let per_worker = total_blocks.div_ceil(workers);
        let partials: Vec<Result<Partial, DocError>> = thread::scope(|scope| {
            let handles: Vec<_> = (0..workers)
                .map(|worker| {
                    let range = (worker * per_worker).min(total_blocks)
                        ..((worker + 1) * per_worker).min(total_blocks);
                    let regex = regex.clone();
                    scope.spawn(move || self.search_range(range, &regex, cancel))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err(io::Error::other("Search worker panicked").into()))
                })
                .collect()
        });

        let mut matches = Vec::new();
        let mut total = 0u64;
        for partial in partials {
            let partial = partial?;
            total += partial.total;
            let room = SEARCH_MATCH_CAP.saturating_sub(matches.len());
            matches.extend(partial.matches.into_iter().take(room));
        }
        Ok(LogSearchResult {
            matches,
            total,
            capped: total > SEARCH_MATCH_CAP as u64,
        })
    }

    fn search_range(
        &self, range: Range<usize>, regex: &Regex, cancel: &CancellationToken,
    ) -> Result<Partial, DocError> {
        let mut partial = Partial {
            matches: Vec::new(),
            total: 0,
        };
        let mut file = None;
        let mut scratch = Vec::new();
        for index in range {
            if cancel.is_cancelled() {
                return Err(DocError::Cancelled);
            }
            let block = self.load_block(&mut file, &mut scratch, index)?;
            let first_line = self.blocks[index].first_line;
            for (offset, meta) in block.lines.iter().enumerate() {
                search_line(
                    regex,
                    block.line_text(meta),
                    first_line + offset as u32,
                    &mut partial,
                );
            }
        }
        Ok(partial)
    }

    pub fn text_block(&self, index: usize) -> Result<Bytes, DocError> {
        if index >= self.blocks.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "No such log block").into());
        }
        let block = self.load_block(&mut None, &mut Vec::new(), index)?;
        let mut out = String::with_capacity(block.text.len() + block.lines.len() * 25);
        for meta in &block.lines {
            if let Some(stamp) = meta.ts.and_then(DateTime::from_timestamp_millis) {
                out.push_str(&stamp.to_rfc3339_opts(SecondsFormat::Millis, true));
                out.push(' ');
            }
            out.push_str(block.line_text(meta));
            out.push('\n');
        }
        Ok(Bytes::from(out.into_bytes()))
    }
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

fn search_line(regex: &Regex, text: &str, line: u32, partial: &mut Partial) {
    let mut ascii: Option<bool> = None;
    let mut last_byte = 0usize;
    let mut last_units = 0u32;
    for found in regex.find_iter(text) {
        if found.is_empty() {
            continue;
        }
        partial.total += 1;
        if partial.matches.len() >= SEARCH_MATCH_CAP {
            continue;
        }
        let ascii = *ascii.get_or_insert_with(|| text.is_ascii());
        let (start, end) = if ascii {
            (found.start() as u32, found.end() as u32)
        } else {
            let start = last_units + utf16_len(&text[last_byte..found.start()]);
            let end = start + utf16_len(found.as_str());
            last_byte = found.end();
            last_units = end;
            (start, end)
        };
        partial.matches.push((line, start, end));
    }
}

fn render_line(block: &DecodedBlock, meta: &LineMeta, max_units: usize) -> LogLine {
    let text = block.line_text(meta);
    let spans = block.line_spans(meta);
    let ascii = text.is_ascii();
    let (kept_end, omitted) = if text.len() <= max_units {
        (text.len(), 0)
    } else if ascii {
        (max_units, text.len() - max_units)
    } else {
        let mut units = 0usize;
        let mut cut = text.len();
        for (index, c) in text.char_indices() {
            if units + c.len_utf16() > max_units {
                cut = index;
                break;
            }
            units += c.len_utf16();
        }
        (cut, utf16_len(&text[cut..]) as usize)
    };
    let kept = &text[..kept_end];
    let table: Vec<u32> = if ascii || spans.is_empty() {
        Vec::new()
    } else {
        let mut table = vec![0u32; kept.len() + 1];
        let mut units = 0u32;
        for (index, c) in kept.char_indices() {
            table[index] = units;
            units += c.len_utf16() as u32;
        }
        table[kept.len()] = units;
        table
    };
    let to_units = |byte: usize| if ascii { byte as u32 } else { table[byte] };
    let spans = spans
        .iter()
        .filter(|s| (s.start as usize) < kept_end)
        .map(|s| {
            let end = (s.start as usize + s.len as usize).min(kept_end);
            (to_units(s.start as usize), to_units(end), s.style)
        })
        .collect();
    LogLine {
        text: kept.to_string(),
        kind: meta.kind,
        ts: meta.ts,
        spans,
        omitted: omitted as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{
        cache,
        ingest,
        small_limits,
    };
    use super::super::{
        BlockLimits,
        MAX_LINE_BYTES,
    };
    use super::*;
    use crate::domain::LineKind;

    fn sample() -> (String, Vec<(String, LineKind)>) {
        let mut input = String::new();
        let mut expected = Vec::new();
        for i in 0..40 {
            let stamp = format!("2026-01-01T10:00:{i:02}.000Z ");
            let (raw, emitted) = match i {
                2 => (
                    "##[group]Outer".to_string(),
                    Some(("Outer".to_string(), LineKind::Group)),
                ),
                3 => (
                    "##[group]Inner".to_string(),
                    Some(("Inner".to_string(), LineKind::Group)),
                ),
                5 | 6 => ("##[endgroup]".to_string(), None),
                10 | 20 | 30 => (
                    format!("##[error]boom {i}"),
                    Some((format!("boom {i}"), LineKind::Error)),
                ),
                21 => (
                    "\u{1b}[31mred \u{e9}\u{1f600}\u{1b}[0m tail".to_string(),
                    Some(("red \u{e9}\u{1f600} tail".to_string(), LineKind::Plain)),
                ),
                22 => (
                    "a\rb\rfinal".to_string(),
                    Some(("final".to_string(), LineKind::Plain)),
                ),
                35 => (
                    "##[group]Unclosed".to_string(),
                    Some(("Unclosed".to_string(), LineKind::Group)),
                ),
                _ => {
                    let text = format!("plain line {i} \u{fc}n\u{ef}code");
                    (text.clone(), Some((text, LineKind::Plain)))
                }
            };
            input.push_str(&stamp);
            input.push_str(&raw);
            input.push_str(if i % 2 == 0 { "\r\n" } else { "\n" });
            expected.extend(emitted);
        }
        (input, expected)
    }

    fn doc_from(lines: &[&str], limits: BlockLimits) -> (tempfile::TempDir, Document) {
        let dir = tempfile::tempdir().expect("tempdir");
        let data: String = lines.iter().map(|l| format!("{l}\n")).collect();
        ingest(dir.path(), data.as_bytes(), 1 << 16, Vec::new(), limits);
        let doc = Document::open(dir.path()).expect("open");
        (dir, doc)
    }

    fn query(text: &str, regex: bool, case_sensitive: bool) -> Regex {
        search_regex(&LogSearchQuery {
            text: text.into(),
            regex,
            case_sensitive,
        })
        .expect("pattern")
        .expect("non-empty")
    }

    fn run(doc: &Document, regex: &Regex) -> LogSearchResult {
        doc.search(regex, &CancellationToken::new())
            .expect("search")
    }

    #[test]
    fn run_logs_document_reads_exact_text_across_blocks() {
        let (input, expected) = sample();
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = ingest(dir.path(), input.as_bytes(), 7, Vec::new(), small_limits());
        assert!(!outcome.truncated);
        let meta = &outcome.meta;
        assert_eq!(meta.line_count as usize, expected.len());
        assert_eq!(meta.line_count, 38);
        assert_eq!(meta.error_total, 3);
        let error_lines: Vec<u32> = expected
            .iter()
            .enumerate()
            .filter(|(_, (_, kind))| *kind == LineKind::Error)
            .map(|(i, _)| i as u32)
            .collect();
        assert_eq!(meta.error_lines, error_lines);
        assert_eq!(meta.sections.len(), 3);
        let unclosed = meta
            .sections
            .iter()
            .find(|s| s.title == "Unclosed")
            .expect("unclosed");
        assert_eq!(unclosed.end_line, 38);
        let inner = meta
            .sections
            .iter()
            .find(|s| s.title == "Inner")
            .expect("inner");
        assert_eq!(
            inner.parent,
            meta.sections
                .iter()
                .find(|s| s.title == "Outer")
                .map(|s| s.id)
        );

        let doc = Document::open(dir.path()).expect("open");
        assert!(doc.block_count() >= 9);
        let cache = cache();
        let page = doc.read_lines(&cache, 10, 15, 16384).expect("page");
        assert_eq!(page.start, 10);
        assert_eq!(page.lines.len(), 15);
        for (line, (text, kind)) in page.lines.iter().zip(&expected[10..25]) {
            assert_eq!(&line.text, text);
            assert_eq!(&line.kind, kind);
            assert!(line.ts.is_some() || text == "final");
        }
        let all = doc.read_lines(&cache, 0, 5000, 16384).expect("all");
        assert_eq!(all.lines.len(), 38);
        for (line, (text, kind)) in all.lines.iter().zip(&expected) {
            assert_eq!((&line.text, &line.kind), (text, kind));
        }
        let red = expected
            .iter()
            .position(|(t, _)| t.starts_with("red"))
            .expect("red");
        assert_eq!(all.lines[red].spans, vec![(0, 7, 0)]);
        assert!(doc
            .read_lines(&cache, 38, 10, 4096)
            .expect("past end")
            .lines
            .is_empty());
    }

    #[test]
    fn run_logs_document_cuts_long_lines_by_utf16_units() {
        let ascii = "x".repeat(1000);
        let wide = "\u{e9}".repeat(1000);
        let emoji = format!("\u{1b}[31m{}\u{1b}[0m", "\u{1f600}".repeat(300));
        let lines = [ascii.as_str(), wide.as_str(), emoji.as_str()];
        let (_dir, doc) = doc_from(&lines, BlockLimits::default());
        let page = doc.read_lines(&cache(), 0, 3, 256).expect("page");
        assert_eq!(page.lines[0].text.len(), 256);
        assert_eq!(page.lines[0].omitted, 744);
        assert_eq!(page.lines[1].text.chars().count(), 256);
        assert_eq!(page.lines[1].omitted, 744);
        assert_eq!(page.lines[2].text.chars().count(), 128);
        assert_eq!(page.lines[2].omitted, 600 - 256);
        assert_eq!(page.lines[2].spans, vec![(0, 256, 0)]);
        let full = doc.read_lines(&cache(), 0, 1, 1 << 20).expect("full");
        assert_eq!(full.lines[0].omitted, 0);
    }

    #[test]
    fn run_logs_document_splits_giant_line_on_char_boundaries() {
        let giant = "\u{20ac}".repeat(1_048_576);
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = ingest(
            dir.path(),
            giant.as_bytes(),
            1 << 16,
            Vec::new(),
            BlockLimits::default(),
        );
        assert!(outcome.meta.line_count >= 3);
        let doc = Document::open(dir.path()).expect("open");
        let cache = cache();
        let mut joined = String::new();
        let mut start = 0;
        while start < outcome.meta.line_count {
            let page = doc.read_lines(&cache, start, 10, 1 << 20).expect("page");
            for line in &page.lines {
                assert!(line.text.len() <= MAX_LINE_BYTES);
                assert_eq!(line.omitted, 0);
                joined.push_str(&line.text);
            }
            start += page.lines.len() as u32;
        }
        assert_eq!(joined, giant);
    }

    #[test]
    fn run_logs_document_truncates_at_text_limit() {
        let lines: Vec<String> = (0..50).map(|_| "0123456789".to_string()).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let limits = BlockLimits {
            max_doc_text_bytes: 100,
            ..small_limits()
        };
        let (dir, doc) = doc_from(&refs, limits);
        assert!(doc.meta().truncated);
        assert_eq!(doc.meta().line_count, 9);
        assert_eq!(doc.meta().text_bytes, 99);
        let page = doc.read_lines(&cache(), 0, 100, 4096).expect("page");
        assert_eq!(page.lines.len(), 9);
        drop(dir);
    }

    #[test]
    fn run_logs_document_empty_input_is_readable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = ingest(dir.path(), b"", 16, Vec::new(), BlockLimits::default());
        assert_eq!(outcome.meta.line_count, 0);
        let doc = Document::open(dir.path()).expect("open");
        assert_eq!(doc.block_count(), 0);
        assert!(doc
            .read_lines(&cache(), 0, 10, 4096)
            .expect("page")
            .lines
            .is_empty());
        assert_eq!(run_empty(&doc), LogSearchResult::default());
    }

    fn run_empty(doc: &Document) -> LogSearchResult {
        run(doc, &query("x", false, true))
    }

    #[test]
    fn run_logs_document_rejects_corrupt_files() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (dir, _) = doc_from(&refs, small_limits());
        let index_path = dir.path().join(INDEX_FILE);
        let index = fs::read(&index_path).expect("index");
        let blocks_path = dir.path().join(BLOCKS_FILE);
        let blocks = fs::read(&blocks_path).expect("blocks");

        let mut longer = index.clone();
        longer.push(0);
        fs::write(&index_path, longer).expect("write");
        assert!(matches!(Document::open(dir.path()), Err(DocError::Corrupt)));

        let mut shifted = index.clone();
        shifted[12] = 1;
        fs::write(&index_path, shifted).expect("write");
        assert!(matches!(Document::open(dir.path()), Err(DocError::Corrupt)));

        fs::write(&index_path, &index[..INDEX_RECORD_BYTES]).expect("write");
        let one_block = Document::open(dir.path()).expect("open");
        assert_eq!(one_block.block_count(), 1);
        fs::write(&index_path, &index).expect("write");

        fs::write(&blocks_path, &blocks[..blocks.len() - 1]).expect("write");
        assert!(matches!(Document::open(dir.path()), Err(DocError::Corrupt)));

        fs::write(&blocks_path, vec![0u8; blocks.len()]).expect("write");
        let doc = Document::open(dir.path()).expect("open");
        assert!(matches!(
            doc.read_lines(&cache(), 0, 5, 4096),
            Err(DocError::Corrupt)
        ));
        assert!(matches!(doc.text_block(0), Err(DocError::Corrupt)));
    }

    #[test]
    fn run_logs_search_matches_in_order_across_blocks() {
        let lines = [
            "start",
            "an Error here",
            "x",
            "y",
            "z",
            "error again",
            "x",
            "y",
            "z",
            "ERROR end",
            "x",
            "y",
        ];
        let (_dir, doc) = doc_from(&lines, small_limits());
        assert!(doc.block_count() >= 3);
        let found = run(&doc, &query("error", false, false));
        assert_eq!(found.matches, vec![(1, 3, 8), (5, 0, 5), (9, 0, 5)]);
        assert_eq!((found.total, found.capped), (3, false));
        let sensitive = run(&doc, &query("error", false, true));
        assert_eq!(sensitive.matches, vec![(5, 0, 5)]);
    }

    #[test]
    fn run_logs_search_regex_and_anchors() {
        let (_dir, doc) = doc_from(
            &["err", "error", "xx", "boom", "boomerang", "a boom", "boom"],
            small_limits(),
        );
        let found = run(&doc, &query("err(or)?", true, true));
        assert_eq!(found.matches, vec![(0, 0, 3), (1, 0, 5)]);
        let exact = run(&doc, &query("^boom$", true, true));
        assert_eq!(exact.matches, vec![(3, 0, 4), (6, 0, 4)]);
        let empties = run(&doc, &query("x*", true, true));
        assert_eq!(empties.matches, vec![(2, 0, 2)]);
    }

    #[test]
    fn run_logs_search_rejects_bad_patterns() {
        let bad = |text: &str, regex: bool| {
            search_regex(&LogSearchQuery {
                text: text.into(),
                regex,
                case_sensitive: true,
            })
        };
        assert!(
            matches!(bad("(", true), Err(DomainError::InvalidConfig(m)) if m.starts_with("Invalid search pattern"))
        );
        assert!(matches!(
            bad(r"(?-u:\xA9)", true),
            Err(DomainError::InvalidConfig(_))
        ));
        assert!(bad("(", false).expect("literal").is_some());
        assert!(bad("", true).expect("empty").is_none());
        let long = "a".repeat(SEARCH_MAX_QUERY_CHARS + 1);
        assert!(
            matches!(bad(&long, false), Err(DomainError::InvalidConfig(m)) if m == "Search text is too long")
        );
        assert!(bad(&long[1..], false).is_ok());
    }

    #[test]
    fn run_logs_search_reports_utf16_offsets() {
        let (_dir, doc) = doc_from(&["a\u{e9}\u{1f600}b b", "\u{1f600}"], small_limits());
        assert_eq!(
            run(&doc, &query("b", false, true)).matches,
            vec![(0, 4, 5), (0, 6, 7)]
        );
        assert_eq!(
            run(&doc, &query("\u{1f600}", false, true)).matches,
            vec![(0, 2, 4), (1, 0, 2)]
        );
        assert_eq!(
            run(&doc, &query("\u{e9}", false, true)).matches,
            vec![(0, 1, 2)]
        );
    }

    #[test]
    fn run_logs_search_is_independent_of_block_size() {
        let lines: Vec<String> = (0..100)
            .map(|i| {
                if i % 7 == 0 {
                    format!("caf\u{e9} {i} match")
                } else {
                    format!("noise {i}")
                }
            })
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (_a, small) = doc_from(&refs, small_limits());
        let (_b, large) = doc_from(&refs, BlockLimits::default());
        assert!(small.block_count() > large.block_count());
        let pattern = query("match|noise 9", true, false);
        assert_eq!(run(&small, &pattern), run(&large, &pattern));
        assert_eq!(run(&small, &pattern).total, 24);
    }

    #[test]
    fn run_logs_search_caps_matches_but_counts_all() {
        let lines = vec!["x"; 12_000];
        let (_dir, doc) = doc_from(&lines, BlockLimits::default());
        let found = run(&doc, &query("x", false, true));
        assert_eq!(found.matches.len(), SEARCH_MATCH_CAP);
        assert_eq!(found.total, 12_000);
        assert!(found.capped);
        assert!(found.matches.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn run_logs_search_honours_cancellation() {
        let (_dir, doc) = doc_from(&["a", "b", "c", "d", "e"], small_limits());
        let token = CancellationToken::new();
        token.cancel();
        assert!(matches!(
            doc.search(&query("a", false, true), &token),
            Err(DocError::Cancelled)
        ));
    }

    #[test]
    fn run_logs_document_text_block_renders_timestamps() {
        let input = "2026-01-01T10:00:00.250Z hello\nno stamp\n";
        let dir = tempfile::tempdir().expect("tempdir");
        ingest(
            dir.path(),
            input.as_bytes(),
            5,
            Vec::new(),
            BlockLimits::default(),
        );
        let doc = Document::open(dir.path()).expect("open");
        let text = doc.text_block(0).expect("block");
        assert_eq!(&text[..], b"2026-01-01T10:00:00.250Z hello\nno stamp\n");
        assert!(doc.text_block(1).is_err());
    }
}
