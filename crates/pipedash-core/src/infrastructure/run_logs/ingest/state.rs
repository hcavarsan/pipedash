use std::fs::File;
use std::io::{
    self,
    BufWriter,
    Write,
};
use std::path::Path;

use chrono::Utc;
use memchr::{
    memchr,
    memchr_iter,
};
use sha2::{
    Digest,
    Sha256,
};

use super::super::ansi::{
    self,
    Span,
    StyleTable,
};
use super::super::block::{
    BlockRef,
    BlockWriter,
    INDEX_RECORD_BYTES,
};
use super::super::checkpoint::{
    Checkpoint,
    Resume,
};
use super::super::failure;
use super::super::outline::Outline;
use super::super::store::{
    short_hex,
    StoredDocMeta,
    STORED_FORMAT,
};
use super::super::MAX_LINE_BYTES;
use super::start::Start;
use super::{
    Flow,
    IngestOutcome,
    Ingester,
    BLOCKS_BUFFER_BYTES,
    META_FILE,
};
use crate::domain::{
    LineKind,
    LineMarker,
    LogCursor,
    LogDialect,
};

pub(super) struct State {
    dialect: &'static dyn LogDialect,
    infer_errors: bool,
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
    pub(super) truncated: bool,
    pending: Vec<u8>,
    hasher: Option<Sha256>,
    pub(super) raw_hash: Option<String>,
    pub(super) new_bytes: u64,
    previous: Option<(i64, u32)>,
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
        LineMarker::GroupStart | LineMarker::SectionStart { .. } => LineKind::Group,
        LineMarker::Error => LineKind::Error,
        LineMarker::Warning => LineKind::Warning,
        LineMarker::Notice => LineKind::Notice,
        LineMarker::Debug => LineKind::Debug,
        LineMarker::Command => LineKind::Command,
        LineMarker::Plain | LineMarker::GroupEnd | LineMarker::OpenSection => LineKind::Plain,
    }
}

impl State {
    pub(super) fn new(cfg: &Ingester, start: Start) -> Self {
        Self {
            dialect: cfg.dialect,
            infer_errors: cfg.dialect.infers_errors(),
            max_doc_text_bytes: cfg.limits.max_doc_text_bytes,
            styles: start.styles,
            outline: start.outline,
            writer: BlockWriter::new(cfg.limits.lines, cfg.limits.target_bytes),
            blocks: BufWriter::with_capacity(BLOCKS_BUFFER_BYTES, start.blocks),
            index: BufWriter::new(start.index),
            blocks_len: start.blocks_len,
            index_len: start.index_len,
            text: String::new(),
            spans: Vec::new(),
            line_count: start.line_count,
            text_bytes: start.text_bytes,
            first_ts: start.first_ts,
            truncated: false,
            pending: start.pending,
            hasher: (!cfg.complete && start.previous.is_none()).then(Sha256::new),
            raw_hash: None,
            new_bytes: 0,
            previous: start.previous,
        }
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> io::Result<Flow> {
        self.new_bytes += chunk.len() as u64;
        if let Some(hasher) = self.hasher.as_mut() {
            hasher.update(chunk);
        }
        let mut start = 0;
        for end in memchr_iter(b'\n', chunk) {
            let flow = if self.pending.is_empty() {
                self.complete_line(&chunk[start..end])?
            } else {
                let mut pending = std::mem::take(&mut self.pending);
                pending.extend_from_slice(&chunk[start..end]);
                let flow = self.complete_line(&pending)?;
                pending.clear();
                self.pending = pending;
                flow
            };
            if flow == Flow::Stop {
                return Ok(Flow::Stop);
            }
            start = end + 1;
        }
        self.pending.extend_from_slice(&chunk[start..]);
        if self.pending.len() > MAX_LINE_BYTES {
            let mut pending = std::mem::take(&mut self.pending);
            let (consumed, flow) = self.split_long(&pending)?;
            pending.drain(..consumed);
            self.pending = pending;
            return Ok(flow);
        }

        Ok(Flow::Continue)
    }

    pub(super) fn seal(&mut self) {
        self.raw_hash = self
            .hasher
            .take()
            .map(|hasher| short_hex(&hasher.finalize()));
    }

    pub(super) fn render_tail(&mut self) -> io::Result<()> {
        if self.truncated || self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        self.complete_line(&pending).map(drop)
    }

    pub(super) fn write_checkpoint(
        &mut self, dir: &Path, cursor: Option<LogCursor>, steps_hash: &str,
    ) -> io::Result<()> {
        let resume = match cursor {
            Some(cursor) => {
                if self.writer.lines() > 0 {
                    self.flush_block()?;
                }
                Some(Resume {
                    cursor,
                    line_count: self.line_count,
                    text_bytes: self.text_bytes,
                    first_ts: self.first_ts,
                    blocks_len: self.blocks_len,
                    index_len: self.index_len,
                    styles: self.styles.len(),
                    outline: &self.outline,
                })
            }
            None => None,
        };
        let tail: &[u8] = if resume.is_some() { &self.pending } else { &[] };

        Checkpoint {
            steps_hash: steps_hash.to_string(),
            raw_hash: self.raw_hash.clone(),
            resume,
        }
        .write(dir, tail)
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
        let visible = clean_carriage_returns(raw);
        let mut parsed = self.dialect.parse(visible);
        let mut timestamp = parsed.timestamp;
        if visible.as_ptr() != raw.as_ptr() {
            let whole = self.dialect.parse(raw);
            if parsed.marker == LineMarker::Plain && whole.marker != LineMarker::Plain {
                parsed = whole;
            }
            timestamp = timestamp.or(whole.timestamp);
        }
        let ts = timestamp.map(|t| t.timestamp_millis());
        match parsed.marker {
            LineMarker::GroupEnd => {
                self.outline.group_end(self.line_count, ts);
                if parsed.body.trim_ascii().is_empty() {
                    return Ok(Flow::Continue);
                }
            }
            LineMarker::OpenSection => {
                self.render(parsed.body);
                self.outline.open_current_group(&self.text);
                return Ok(Flow::Continue);
            }
            LineMarker::SectionStart { .. } => self.outline.end_groups(self.line_count),
            _ => {}
        }
        self.render(parsed.body);
        let added = self.text.len() as u64 + 1;
        if self.text_bytes + added > self.max_doc_text_bytes {
            self.truncated = true;
            return Ok(Flow::Stop);
        }
        let kind = match kind_of(parsed.marker) {
            LineKind::Plain if self.infer_errors && failure::is_error(&self.text) => {
                LineKind::Error
            }
            kind => kind,
        };
        let step_start = self.dialect.is_step_start(&parsed);
        self.outline
            .line(self.line_count, kind, ts, &self.text, step_start)?;
        if parsed.marker == (LineMarker::SectionStart { open: true }) {
            self.outline.open_current_group("");
        }
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

    fn render(&mut self, body: &[u8]) {
        self.text.clear();
        self.spans.clear();
        ansi::render(body, &mut self.styles, &mut self.text, &mut self.spans);
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
        self.index_len += INDEX_RECORD_BYTES as u64;
        Ok(())
    }

    pub(super) fn finish(mut self, cfg: Ingester, resumable: bool) -> io::Result<IngestOutcome> {
        if self.writer.lines() > 0 {
            self.flush_block()?;
        }
        let outline = self.outline.finish(self.line_count)?;
        if !resumable {
            self.outline.discard_spool()?;
        }
        self.blocks
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;
        self.index
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;

        let (previous_revision, stable_lines) = match self.previous {
            Some((revision, lines)) => (Some(revision), lines),
            None => (None, 0),
        };
        let meta = StoredDocMeta {
            format: STORED_FORMAT,
            key: cfg.key,
            revision: cfg.revision,
            complete: cfg.complete,
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
            updated_at_ms: Utc::now().timestamp_millis(),
            stable_lines,
            previous_revision,
        };
        let file = File::create(cfg.dir.join(META_FILE))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, &meta).map_err(io::Error::from)?;
        writer
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;
        Ok(IngestOutcome {
            truncated: self.truncated,
            resumable,
            meta,
        })
    }
}
