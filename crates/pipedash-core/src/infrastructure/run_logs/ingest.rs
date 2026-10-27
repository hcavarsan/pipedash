use std::fs::File;
use std::io::{
    self,
    BufWriter,
    Write,
};
use std::path::PathBuf;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::Arc;

use bytes::Bytes;
use memchr::{
    memchr,
    memchr_iter,
};
use tokio::sync::mpsc;

use super::ansi::{
    self,
    Span,
    StyleTable,
};
use super::block::{
    BlockRef,
    BlockWriter,
};
use super::outline::{
    Outline,
    StepBound,
};
use super::store::{
    DocKey,
    StoredDocMeta,
};
use super::{
    BLOCK_MAX_LINES,
    BLOCK_TARGET_BYTES,
    MAX_DOC_TEXT_BYTES,
    MAX_LINE_BYTES,
};
use crate::domain::{
    LineKind,
    LineMarker,
    LogDialect,
};

pub(super) const BLOCKS_FILE: &str = "blocks.bin";
pub(super) const INDEX_FILE: &str = "index.bin";
pub(super) const META_FILE: &str = "meta.json";
const BLOCKS_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BlockLimits {
    pub lines: usize,
    pub target_bytes: usize,
    pub max_doc_text_bytes: u64,
}

impl Default for BlockLimits {
    fn default() -> Self {
        Self {
            lines: BLOCK_MAX_LINES,
            target_bytes: BLOCK_TARGET_BYTES,
            max_doc_text_bytes: MAX_DOC_TEXT_BYTES,
        }
    }
}

pub struct Ingester {
    pub key: DocKey,
    pub revision: i64,
    pub complete: bool,
    pub dir: PathBuf,
    pub dialect: &'static dyn LogDialect,
    pub steps: Vec<StepBound>,
    pub received: Arc<AtomicU64>,
    pub limits: BlockLimits,
}

#[derive(Debug)]
pub struct IngestOutcome {
    pub meta: StoredDocMeta,
    pub truncated: bool,
}

#[derive(PartialEq, Eq)]
enum Flow {
    Continue,
    Stop,
}

impl Ingester {
    pub fn run(self, rx: &mut mpsc::Receiver<Bytes>) -> io::Result<IngestOutcome> {
        let received = self.received.clone();
        let mut state = State::new(self)?;
        let mut pending: Vec<u8> = Vec::new();
        'receive: while let Some(chunk) = rx.blocking_recv() {
            received.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            let mut start = 0;
            for end in memchr_iter(b'\n', &chunk) {
                let flow = if pending.is_empty() {
                    state.complete_line(&chunk[start..end])?
                } else {
                    pending.extend_from_slice(&chunk[start..end]);
                    let flow = state.complete_line(&pending)?;
                    pending.clear();
                    flow
                };
                if flow == Flow::Stop {
                    break 'receive;
                }
                start = end + 1;
            }
            pending.extend_from_slice(&chunk[start..]);
            if pending.len() > MAX_LINE_BYTES {
                let (consumed, flow) = state.split_long(&pending)?;
                pending.drain(..consumed);
                if flow == Flow::Stop {
                    break;
                }
            }
        }
        if state.truncated {
            rx.close();
        } else if !pending.is_empty() {
            state.complete_line(&pending)?;
        }
        state.finish()
    }
}

struct State {
    key: DocKey,
    revision: i64,
    complete: bool,
    dir: PathBuf,
    dialect: &'static dyn LogDialect,
    max_doc_text_bytes: u64,
    styles: StyleTable,
    outline: Outline,
    writer: BlockWriter,
    blocks: BufWriter<File>,
    index: BufWriter<File>,
    blocks_len: u64,
    index_len: u64,
    text: String,
    spans: Vec<Span>,
    line_count: u32,
    text_bytes: u64,
    first_ts: Option<i64>,
    truncated: bool,
}

fn char_cut(buf: &[u8]) -> usize {
    let mut cut = MAX_LINE_BYTES;
    while cut > 0 && buf[cut] & 0xC0 == 0x80 {
        cut -= 1;
    }
    if cut == 0 {
        MAX_LINE_BYTES
    } else {
        cut
    }
}

fn clean_carriage_returns(raw: &[u8]) -> &[u8] {
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    if memchr(b'\r', raw).is_none() {
        return raw;
    }
    raw.rsplit(|&b| b == b'\r')
        .find(|segment| !segment.is_empty())
        .unwrap_or_default()
}

fn kind_of(marker: LineMarker) -> LineKind {
    match marker {
        LineMarker::GroupStart => LineKind::Group,
        LineMarker::Error => LineKind::Error,
        LineMarker::Warning => LineKind::Warning,
        LineMarker::Notice => LineKind::Notice,
        LineMarker::Debug => LineKind::Debug,
        LineMarker::Command => LineKind::Command,
        LineMarker::Plain | LineMarker::GroupEnd => LineKind::Plain,
    }
}

impl State {
    fn new(ingester: Ingester) -> io::Result<Self> {
        let blocks = File::create(ingester.dir.join(BLOCKS_FILE))?;
        let index = File::create(ingester.dir.join(INDEX_FILE))?;
        Ok(Self {
            outline: Outline::new(ingester.steps, &ingester.dir),
            writer: BlockWriter::new(ingester.limits.lines, ingester.limits.target_bytes),
            key: ingester.key,
            revision: ingester.revision,
            complete: ingester.complete,
            dir: ingester.dir,
            dialect: ingester.dialect,
            max_doc_text_bytes: ingester.limits.max_doc_text_bytes,
            styles: StyleTable::default(),
            blocks: BufWriter::with_capacity(BLOCKS_BUFFER_BYTES, blocks),
            index: BufWriter::new(index),
            blocks_len: 0,
            index_len: 0,
            text: String::new(),
            spans: Vec::new(),
            line_count: 0,
            text_bytes: 0,
            first_ts: None,
            truncated: false,
        })
    }

    fn split_long(&mut self, raw: &[u8]) -> io::Result<(usize, Flow)> {
        let mut offset = 0;
        while raw.len() - offset > MAX_LINE_BYTES {
            let cut = char_cut(&raw[offset..]);
            let flow = self.line(&raw[offset..offset + cut])?;
            offset += cut;
            if flow == Flow::Stop {
                return Ok((offset, Flow::Stop));
            }
        }
        Ok((offset, Flow::Continue))
    }

    fn complete_line(&mut self, raw: &[u8]) -> io::Result<Flow> {
        let (offset, flow) = self.split_long(raw)?;
        if flow == Flow::Stop {
            return Ok(Flow::Stop);
        }
        self.line(&raw[offset..])
    }

    fn line(&mut self, raw: &[u8]) -> io::Result<Flow> {
        let parsed = self.dialect.parse(clean_carriage_returns(raw));
        let ts = parsed.timestamp.map(|t| t.timestamp_millis());
        if parsed.marker == LineMarker::GroupEnd {
            self.outline.group_end(self.line_count, ts);
            return Ok(Flow::Continue);
        }
        self.text.clear();
        self.spans.clear();
        ansi::render(
            parsed.body,
            &mut self.styles,
            &mut self.text,
            &mut self.spans,
        );
        let added = self.text.len() as u64 + 1;
        if self.text_bytes + added > self.max_doc_text_bytes {
            self.truncated = true;
            return Ok(Flow::Stop);
        }
        let kind = kind_of(parsed.marker);
        let step_start = self.dialect.is_step_start(&parsed);
        self.outline
            .line(self.line_count, kind, ts, &self.text, step_start)?;
        if self.first_ts.is_none() {
            self.first_ts = ts;
        }
        if self
            .writer
            .would_overflow(self.text.len(), self.spans.len())
        {
            self.flush_block()?;
        }
        self.writer.push(kind, ts, &self.text, &self.spans);
        self.text_bytes += added;
        self.line_count += 1;
        Ok(Flow::Continue)
    }

    fn flush_block(&mut self) -> io::Result<()> {
        let first_line = self.line_count - self.writer.lines() as u32;
        let data = self.writer.finish();
        let compressed_len = u32::try_from(data.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Log block is too large"))?;
        let record = BlockRef {
            offset: self.blocks_len,
            compressed_len,
            first_line,
        };
        self.blocks.write_all(&data)?;
        self.index.write_all(&record.to_bytes())?;
        self.blocks_len += data.len() as u64;
        self.index_len += super::block::INDEX_RECORD_BYTES as u64;
        Ok(())
    }

    fn finish(mut self) -> io::Result<IngestOutcome> {
        if self.writer.lines() > 0 {
            self.flush_block()?;
        }
        let outline = self.outline.finish(self.line_count)?;
        self.blocks
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;
        self.index
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;

        let meta = StoredDocMeta {
            format: 1,
            key: self.key,
            revision: self.revision,
            complete: self.complete,
            truncated: self.truncated,
            line_count: self.line_count,
            text_bytes: self.text_bytes,
            disk_bytes: self.blocks_len + self.index_len,
            first_timestamp_ms: self.first_ts,
            styles: self.styles.into_list(),
            sections: outline.sections,
            error_lines: outline.error_lines,
            warning_lines: outline.warning_lines,
            error_total: outline.error_total,
            warning_total: outline.warning_total,
            focus_line: outline.focus_line,
        };
        let file = File::create(self.dir.join(META_FILE))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, &meta).map_err(io::Error::from)?;
        writer
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;
        Ok(IngestOutcome {
            truncated: self.truncated,
            meta,
        })
    }
}
