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
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use bytes::Bytes;
use chrono::{
    DateTime,
    SecondsFormat,
};
use thiserror::Error;

use self::cache::BlockCache;
use self::search::utf16_len;
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
use super::store::{
    StoredDocMeta,
    STORED_FORMAT,
};
use super::{
    BLOCK_MAX_RAW_BYTES,
    LINE_UNITS_MAX,
    LINE_UNITS_MIN,
    PAGE_MAX_LINES,
    PAGE_MAX_TEXT_BYTES,
};
use crate::domain::{
    LogLine,
    LogLinesPage,
};

pub(super) mod cache;
pub(super) mod search;
#[cfg(test)]
mod tests;

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

pub struct Document {
    dir: PathBuf,
    meta: Arc<StoredDocMeta>,
    blocks: Arc<[BlockRef]>,
}

impl Document {
    pub fn open(dir: &Path) -> Result<Self, DocError> {
        let meta: StoredDocMeta = serde_json::from_slice(&fs::read(dir.join(META_FILE))?)
            .map_err(|_| DocError::Corrupt)?;
        if meta.format != STORED_FORMAT {
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

    pub(super) fn dir(&self) -> &Path {
        &self.dir
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
