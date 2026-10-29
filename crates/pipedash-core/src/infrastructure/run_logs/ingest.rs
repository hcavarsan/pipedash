use std::fs::{
    self,
    File,
};
use std::io::{
    self,
    BufWriter,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::Arc;

use bytes::Bytes;
use chrono::Utc;
use memchr::{
    memchr,
    memchr_iter,
};
use sha2::{
    Digest,
    Sha256,
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
use super::checkpoint::{
    self,
    copy_prefix,
    steps_hash,
    Base,
    Checkpoint,
    Resume,
    TAIL_FILE,
};
use super::failure;
use super::outline::{
    Outline,
    StepBound,
};
use super::store::{
    short_hex,
    DocKey,
    StoredDocMeta,
    STORED_FORMAT,
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
    LogCursor,
    LogDialect,
    LogRead,
    LogReadMode,
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
    pub base: Option<Base>,
    pub restart: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct IngestOutcome {
    pub meta: StoredDocMeta,
    pub truncated: bool,
    pub resumable: bool,
}

#[derive(PartialEq, Eq)]
enum Flow {
    Continue,
    Stop,
}

pub struct Ingested {
    cfg: Ingester,
    resumed: Option<bool>,
    state: Option<State>,
    steps_hash: String,
}

impl Ingester {
    pub fn run(self, rx: &mut mpsc::Receiver<Bytes>) -> io::Result<Ingested> {
        let received = Arc::clone(&self.received);
        let mut ingested = Ingested {
            steps_hash: steps_hash(&self.steps),
            cfg: self,
            resumed: None,
            state: None,
        };
        while let Some(chunk) = rx.blocking_recv() {
            received.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            if ingested.start()?.push(&chunk)? == Flow::Stop {
                break;
            }
        }
        if !ingested.decide() {
            ingested.start()?;
        }
        if let Some(state) = ingested.state.as_mut() {
            state.seal();
            if state.truncated {
                rx.close();
            }
        }

        Ok(ingested)
    }
}

impl Ingested {
    pub fn resumed(&self) -> bool {
        self.resumed == Some(true)
    }

    pub fn truncated(&self) -> bool {
        self.state.as_ref().is_some_and(|state| state.truncated)
    }

    pub fn unchanged(&self, read: &LogRead) -> bool {
        let Some(base) = &self.cfg.base else {
            return false;
        };
        let Some(prior) = &base.checkpoint else {
            return false;
        };
        if base.document.meta().complete != self.cfg.complete || prior.steps_hash != self.steps_hash
        {
            return false;
        }
        let state = self.state.as_ref();
        match read.mode {
            LogReadMode::Append => {
                state.is_none_or(|state| state.new_bytes == 0)
                    && read.cursor.as_ref().is_some_and(|cursor| {
                        prior
                            .resume
                            .as_ref()
                            .is_some_and(|resume| &resume.cursor == cursor)
                    })
            }
            LogReadMode::Replace => {
                let hash = state.and_then(|state| state.raw_hash.as_deref());
                hash.is_some() && hash == prior.raw_hash.as_deref()
            }
        }
    }

    pub fn finish(mut self, cursor: Option<LogCursor>) -> io::Result<IngestOutcome> {
        self.start()?;
        let Self {
            cfg,
            state,
            steps_hash,
            ..
        } = self;
        let mut state = state.ok_or_else(not_started)?;
        let had_cursor = cursor.is_some();
        let mut keep =
            !cfg.complete && !state.truncated && (had_cursor || state.raw_hash.is_some());
        if keep {
            state.write_checkpoint(&cfg.dir, cursor, &steps_hash)?;
        }
        state.render_tail()?;
        if keep && state.truncated {
            checkpoint::remove(&cfg.dir);
            keep = false;
        }

        state.finish(cfg, keep && had_cursor)
    }

    fn decide(&mut self) -> bool {
        let Self { cfg, resumed, .. } = self;
        *resumed.get_or_insert_with(|| {
            cfg.base
                .as_ref()
                .is_some_and(|base| base.cursor(&cfg.steps).is_some())
                && !cfg.restart.load(Ordering::SeqCst)
        })
    }

    fn start(&mut self) -> io::Result<&mut State> {
        if self.state.is_none() {
            let start = if self.decide() {
                Start::resume(&mut self.cfg)?
            } else {
                Start::fresh(&mut self.cfg)?
            };
            self.state = Some(State::new(&self.cfg, start));
        }

        self.state.as_mut().ok_or_else(not_started)
    }
}

fn not_started() -> io::Error {
    io::Error::other("The log ingest has not started")
}

struct Start {
    outline: Outline,
    styles: StyleTable,
    blocks: File,
    index: File,
    blocks_len: u64,
    index_len: u64,
    line_count: u32,
    text_bytes: u64,
    first_ts: Option<i64>,
    pending: Vec<u8>,
    previous: Option<(i64, u32)>,
}

impl Start {
    fn fresh(cfg: &mut Ingester) -> io::Result<Self> {
        Ok(Self {
            outline: Outline::new(std::mem::take(&mut cfg.steps), &cfg.dir),
            styles: StyleTable::default(),
            blocks: File::create(cfg.dir.join(BLOCKS_FILE))?,
            index: File::create(cfg.dir.join(INDEX_FILE))?,
            blocks_len: 0,
            index_len: 0,
            line_count: 0,
            text_bytes: 0,
            first_ts: None,
            pending: Vec::new(),
            previous: None,
        })
    }

    fn resume(cfg: &mut Ingester) -> io::Result<Self> {
        let missing = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "There is no log checkpoint to resume from",
            )
        };
        let Base {
            revision,
            document,
            checkpoint,
            ..
        } = cfg.base.as_mut().ok_or_else(missing)?;
        let resume = checkpoint
            .as_mut()
            .and_then(|checkpoint| checkpoint.resume.as_mut())
            .ok_or_else(missing)?;
        let from = document.dir();
        let pending = match fs::read(from.join(TAIL_FILE)) {
            Ok(tail) => tail,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };

        Ok(Self {
            blocks: copy_prefix(
                &from.join(BLOCKS_FILE),
                &cfg.dir.join(BLOCKS_FILE),
                resume.blocks_len,
            )?,
            index: copy_prefix(
                &from.join(INDEX_FILE),
                &cfg.dir.join(INDEX_FILE),
                resume.index_len,
            )?,
            outline: std::mem::take(&mut resume.outline).resume(
                std::mem::take(&mut cfg.steps),
                from,
                &cfg.dir,
            )?,
            styles: StyleTable::from_list(document.meta().styles[..resume.styles].to_vec()),
            blocks_len: resume.blocks_len,
            index_len: resume.index_len,
            line_count: resume.line_count,
            text_bytes: resume.text_bytes,
            first_ts: resume.first_ts,
            pending,
            previous: Some((*revision, resume.line_count)),
        })
    }
}

struct State {
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
    truncated: bool,
    pending: Vec<u8>,
    hasher: Option<Sha256>,
    raw_hash: Option<String>,
    new_bytes: u64,
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
    fn new(cfg: &Ingester, start: Start) -> Self {
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

    fn push(&mut self, chunk: &[u8]) -> io::Result<Flow> {
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

    fn seal(&mut self) {
        self.raw_hash = self
            .hasher
            .take()
            .map(|hasher| short_hex(&hasher.finalize()));
    }

    fn render_tail(&mut self) -> io::Result<()> {
        if self.truncated || self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        self.complete_line(&pending).map(drop)
    }

    fn write_checkpoint(
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
        self.index_len += super::block::INDEX_RECORD_BYTES as u64;
        Ok(())
    }

    fn finish(mut self, cfg: Ingester, resumable: bool) -> io::Result<IngestOutcome> {
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

#[cfg(test)]
mod tests {
    use pipedash_plugin_api::PlainDialect;
    use pipedash_plugin_azure_devops::AzureDevOpsLogDialect;
    use pipedash_plugin_bitbucket::BitbucketLogDialect;
    use pipedash_plugin_buildkite::BuildkiteLogDialect;
    use pipedash_plugin_gitea::GiteaLogDialect;
    use pipedash_plugin_gitlab::GitlabLogDialect;
    use pipedash_plugin_jenkins::JenkinsLogDialect;
    use pipedash_plugin_tekton::TektonLogDialect;

    use super::super::document::Document;
    use super::super::testing::{
        doc_key,
        ingester,
    };
    use super::*;

    static BUILDKITE: BuildkiteLogDialect = BuildkiteLogDialect;

    const BUILDKITE_LOG: &str = concat!(
        "\x1b_bk;t=1745322209921\x07~~~ Running global environment hook\r\n",
        "\x1b_bk;t=1745322209921\x07\x1b[90m$\x1b[0m /buildkite/agent/hooks/environment\r\n",
        "\x1b_bk;t=1745322210692\x07~~~ Running script\r\n",
        "\x1b_bk;t=1745322210692\x07\x1b[90m$\x1b[0m ./script.sh\r\n",
        "\x1b_bk;t=1745322210694\x07--- :package: Build\r\r\n",
        "\x1b_bk;t=1745322210698\x07remote: Counting objects:  50%\x1b[K",
        "\x1b_bk;t=1745322210699\x07\rremote: Counting objects: 100%, done.\x1b[K",
        "\x1b_bk;t=1745322210700\x07\r\r\n",
        "\x1b_bk;t=1745322210800\x07+++ Running tests\r\n",
        "\x1b_bk;t=1745322210900\x07ok\r\n",
        "\x1b_bk;t=1745322211000\x07--- Lint\r\n",
        "\x1b_bk;t=1745322212000\x07\x1b[31m🚨 Error: The command exited with status 1\x1b[0m\r\n",
        "\x1b_bk;t=1745322212000\x07^^^ +++\r\n",
        "\x1b_bk;t=1745322212100\x07--- Upload\r\n",
        "\x1b_bk;t=1745322212200\x07^^^ +++ Upload failed\r\n",
    );

    #[test]
    fn buildkite_groups_become_flat_sections() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ingester = ingester(dir.path(), doc_key(), 1, Vec::new(), BlockLimits::default());
        ingester.dialect = &BUILDKITE;
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send(Bytes::from_static(BUILDKITE_LOG.as_bytes()))
            .expect("channel has room");
        drop(tx);

        let meta = ingester
            .run(&mut rx)
            .expect("ingest")
            .finish(None)
            .expect("finish")
            .meta;

        let sections: Vec<_> = meta
            .sections
            .iter()
            .map(|s| {
                (
                    s.title.as_str(),
                    s.parent,
                    s.header_line,
                    s.end_line,
                    s.open,
                )
            })
            .collect();
        assert_eq!(
            sections,
            vec![
                ("Running global environment hook", None, Some(0), 2, false),
                ("Running script", None, Some(2), 4, false),
                (":package: Build", None, Some(4), 6, false),
                ("Running tests", None, Some(6), 8, true),
                ("Lint", None, Some(8), 10, true),
                ("Upload failed", None, Some(10), 11, true),
            ]
        );
        assert_eq!(meta.sections[3].duration_ms, Some(100));
        assert_eq!(meta.line_count, 11);
        assert_eq!(meta.error_lines, vec![9]);
        assert_eq!(meta.focus_line, Some(9));
        assert_eq!(meta.first_timestamp_ms, Some(1_745_322_209_921));

        let text = Document::open(dir.path())
            .expect("open")
            .text_block(0)
            .expect("block");
        let text = std::str::from_utf8(&text).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 11);
        assert_eq!(
            lines[5],
            "2025-04-22T11:43:30.698Z remote: Counting objects: 100%, done."
        );
        assert!(lines[9].ends_with(" 🚨 Error: The command exited with status 1"));
        assert!(!text.contains("bk;t=") && !text.contains("^^^"));
    }

    static GITLAB: GitlabLogDialect = GitlabLogDialect;

    const GITLAB_LOG: &str = concat!(
        "Running with gitlab-runner 18.4.0 (139a0ac0)\n",
        "  on code-europa-eu-runner fkXhbgRUT, system ID: s_5247efa18f3c\n",
        "section_start:1790320399:prepare_executor\r\x1b[0K\x1b[0K\x1b[36;1mPreparing the \"docker+machine\" executor\x1b[0;m\x1b[0;m\n",
        "\x1b[0KUsing Docker executor with image python:3.11 ...\x1b[0;m\n",
        "section_end:1790320401:prepare_executor\r\x1b[0Ksection_start:1790320401:prepare_script\r\x1b[0K\x1b[0K\x1b[36;1mPreparing environment\x1b[0;m\x1b[0;m\n",
        "Running on runner-fkxhbgrut-project-1344-concurrent-0\n",
        "section_end:1790320402:prepare_script\r\x1b[0Ksection_start:1790320402:get_sources[collapsed=true]\r\x1b[0K\x1b[0K\x1b[36;1mGetting source from Git repository\x1b[0;m\x1b[0;m\n",
        "\x1b[32;1mFetching changes with git depth set to 20...\x1b[0;m\n",
        "section_end:1790320403:get_sources\r\x1b[0Ksection_start:1790320403:step_script\r\x1b[0K\x1b[0K\x1b[36;1mExecuting \"step_script\" stage of the job script\x1b[0;m\x1b[0;m\n",
        "\x1b[32;1m$ make test\x1b[0;m\n",
        "test result: ok. 3 passed\n",
        "section_end:1790320422:step_script\r\x1b[0Ksection_start:1790320422:cleanup_file_variables\r\x1b[0K\x1b[0K\x1b[36;1mCleaning up project directory and file based variables\x1b[0;m\x1b[0;m\n",
        "section_end:1790320423:cleanup_file_variables\r\x1b[0K\x1b[32;1mJob succeeded\x1b[0;m\n",
    );

    const GITLAB_TIMESTAMPED_LOG: &str = concat!(
        "2026-10-27T10:00:00.100000Z 00O Running with gitlab-runner 17.0.0\n",
        "2026-10-27T10:00:00.100000Z 00O section_start:1793095200:prepare_executor\r\x1b[0K\n",
        "2026-10-27T10:00:00.100100Z 00O+\x1b[0K\x1b[36;1mPreparing the \"shell\" executor\x1b[0;m\x1b[0;m\n",
        "2026-10-27T10:00:01.200000Z 00O \x1b[0KUsing Shell (bash) executor...\x1b[0;m\n",
        "2026-10-27T10:00:02.300000Z 00O section_end:1793095202:prepare_executor\r\x1b[0Ksection_start:1793095202:step_script\r\x1b[0K\n",
        "2026-10-27T10:00:02.300100Z 00O+\x1b[0K\x1b[36;1mExecuting \"step_script\" stage of the job script\x1b[0;m\n",
        "2026-10-27T10:00:03.400000Z 00O progress 10%\rprogress 100%\n",
        "2026-10-27T10:00:04.500000Z 00O section_end:1793095204:step_script\r\x1b[0K\n",
        "2026-10-27T10:00:04.500000Z 00O Job succeeded\n",
    );

    type SectionRow = (String, Option<u32>, Option<u32>, u32, bool);

    fn ingest_gitlab(log: &str) -> (Vec<SectionRow>, Vec<String>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ingester = ingester(dir.path(), doc_key(), 1, Vec::new(), BlockLimits::default());
        ingester.dialect = &GITLAB;
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send(Bytes::copy_from_slice(log.as_bytes()))
            .expect("channel has room");
        drop(tx);

        let meta = ingester
            .run(&mut rx)
            .expect("ingest")
            .finish(None)
            .expect("finish")
            .meta;

        let sections = meta
            .sections
            .iter()
            .map(|s| (s.title.clone(), s.parent, s.header_line, s.end_line, s.open))
            .collect();
        let text = Document::open(dir.path())
            .expect("open")
            .text_block(0)
            .expect("block");
        let lines = std::str::from_utf8(&text)
            .expect("utf8")
            .lines()
            .map(str::to_string)
            .collect();
        (sections, lines)
    }

    #[test]
    fn gitlab_sections_survive_ingest() {
        let (sections, lines) = ingest_gitlab(GITLAB_LOG);

        let row = |title: &str, header: u32, end: u32, open: bool| -> SectionRow {
            (title.to_string(), None, Some(header), end, open)
        };
        assert_eq!(
            sections,
            vec![
                row("Preparing the \"docker+machine\" executor", 2, 4, true),
                row("Preparing environment", 4, 6, true),
                row("Getting source from Git repository", 6, 8, false),
                row(
                    "Executing \"step_script\" stage of the job script",
                    8,
                    11,
                    true
                ),
                row(
                    "Cleaning up project directory and file based variables",
                    11,
                    12,
                    true
                ),
            ]
        );
        assert_eq!(lines.len(), 13);
        assert!(lines[12].ends_with("Job succeeded"));
        assert!(lines.iter().all(|line| !line.contains("section_")));
    }

    #[test]
    fn gitlab_text_after_a_section_end_follows_the_section() {
        let (sections, lines) = ingest_gitlab(concat!(
            "section_start:1700000000:build[collapsed=true]\r\x1b[0KBuild\n",
            "compiling\n",
            "section_end:1700000010:build\r\x1b[0KDone\n",
            "after the section\n",
        ));

        assert_eq!(
            sections,
            vec![("Build".to_string(), None, Some(0), 2, false)]
        );
        assert_eq!(
            lines,
            vec![
                "2023-11-14T22:13:20.000Z Build",
                "compiling",
                "2023-11-14T22:13:30.000Z Done",
                "after the section",
            ]
        );
    }

    #[test]
    fn gitlab_runner_timestamps_and_continued_headers_survive_ingest() {
        let (sections, lines) = ingest_gitlab(GITLAB_TIMESTAMPED_LOG);

        assert_eq!(
            sections,
            vec![
                ("prepare_executor".to_string(), None, Some(1), 4, true),
                ("step_script".to_string(), None, Some(4), 7, true),
            ]
        );
        assert_eq!(lines.len(), 8);
        assert_eq!(
            lines[0],
            "2026-10-27T10:00:00.100Z Running with gitlab-runner 17.0.0"
        );
        assert_eq!(lines[6], "2026-10-27T10:00:03.400Z progress 100%");
        assert_eq!(lines[7], "2026-10-27T10:00:04.500Z Job succeeded");
        assert!(lines.iter().all(|line| !line.contains("00O")));
    }

    static GITEA: GiteaLogDialect = GiteaLogDialect;
    static BITBUCKET: BitbucketLogDialect = BitbucketLogDialect;
    static TEKTON: TektonLogDialect = TektonLogDialect;
    static JENKINS: JenkinsLogDialect = JenkinsLogDialect;
    static AZURE: AzureDevOpsLogDialect = AzureDevOpsLogDialect;
    static PLAIN: PlainDialect = PlainDialect;

    fn error_texts(dialect: &'static dyn LogDialect, log: &str) -> Vec<String> {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ingester = ingester(dir.path(), doc_key(), 1, Vec::new(), BlockLimits::default());
        ingester.dialect = dialect;
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send(Bytes::copy_from_slice(log.as_bytes()))
            .expect("channel has room");
        drop(tx);

        let meta = ingester
            .run(&mut rx)
            .expect("ingest")
            .finish(None)
            .expect("finish")
            .meta;

        let text = Document::open(dir.path())
            .expect("open")
            .text_block(0)
            .expect("block");
        let lines: Vec<&str> = std::str::from_utf8(&text).expect("utf8").lines().collect();
        assert_eq!(meta.error_lines.len() as u32, meta.error_total);
        meta.error_lines
            .iter()
            .map(|&line| lines[line as usize].to_string())
            .collect()
    }

    fn assert_errors(found: &[String], expected: &[&str]) {
        assert_eq!(found.len(), expected.len(), "{found:#?}");
        for (line, suffix) in found.iter().zip(expected) {
            assert!(
                line.ends_with(suffix),
                "{line:?} does not end with {suffix:?}"
            );
        }
    }

    #[test]
    fn jenkins_console_notes_are_dropped_before_search_and_error_inference() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ingester = ingester(dir.path(), doc_key(), 1, Vec::new(), BlockLimits::default());
        ingester.dialect = &JENKINS;
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send(Bytes::from_static(
            concat!(
                "\x1b[8mha:////4IkPeNOtKgYQ==\x1b[0m[Pipeline] Start of Pipeline\n",
                "Started by user \x1b[8mha:////4O3qAAAA=\x1b[0madmin\n",
                "\x1b[8mha:////4IkPeNOtKgYQ==\x1b[0mERROR: Job failed: exit code 1\n",
            )
            .as_bytes(),
        ))
        .expect("channel has room");
        drop(tx);

        let meta = ingester
            .run(&mut rx)
            .expect("ingest")
            .finish(None)
            .expect("finish")
            .meta;

        let text = Document::open(dir.path())
            .expect("open")
            .text_block(0)
            .expect("block");
        assert_eq!(
            std::str::from_utf8(&text).expect("utf8"),
            "[Pipeline] Start of Pipeline\nStarted by user admin\nERROR: Job failed: exit code 1\n"
        );
        assert!(meta.error_lines.is_empty());

        let found = error_texts(
            &PLAIN,
            "\x1b[8mha:////4IkPeNOtKgYQ==\x1b[0mERROR: Job failed: exit code 1\n",
        );
        assert_eq!(found, ["ERROR: Job failed: exit code 1"]);
    }

    #[test]
    fn gitlab_failed_job_flags_the_runner_verdict_and_tool_failures() {
        let found = error_texts(
            &GITLAB,
            concat!(
                "Running with gitlab-runner 18.4.0 (139a0ac0)\n",
                "section_start:1790320399:step_script\r\x1b[0K\x1b[0K\x1b[36;1mExecuting \"step_script\" stage of the job script\x1b[0;m\x1b[0;m\n",
                "\x1b[32;1m$ npm test\x1b[0;m\n",
                "FAIL src/login.test.ts\n",
                "npm ERR! Test failed.  See above for more details.\n",
                "section_end:1790320422:step_script\r\x1b[0Ksection_start:1790320422:cleanup_file_variables\r\x1b[0K\x1b[0K\x1b[36;1mCleaning up project directory and file based variables\x1b[0;m\x1b[0;m\n",
                "section_end:1790320423:cleanup_file_variables\r\x1b[0K\x1b[31;1mERROR: Job failed: exit code 1\x1b[0;m\n",
            ),
        );

        assert_errors(
            &found,
            &[
                "FAIL src/login.test.ts",
                "npm ERR! Test failed.  See above for more details.",
                "ERROR: Job failed: exit code 1",
            ],
        );
    }

    #[test]
    fn gitea_failed_job_flags_the_runner_verdict_and_tool_failures() {
        let found = error_texts(
            &GITEA,
            concat!(
                "2026-10-28T10:15:30.0000000Z ⭐ Run Main npm test\n",
                "2026-10-28T10:15:31.0000000Z > app@1.0.0 test\n",
                "2026-10-28T10:15:31.5000000Z FAIL src/login.test.ts\n",
                "2026-10-28T10:15:32.0000000Z npm ERR! Test failed.  See above for more details.\n",
                "2026-10-28T10:15:33.0000000Z ❌  Failure - Main npm test\n",
                "2026-10-28T10:15:33.0000000Z exitcode '1': failure\n",
            ),
        );

        assert_errors(
            &found,
            &[
                "FAIL src/login.test.ts",
                "npm ERR! Test failed.  See above for more details.",
                "❌  Failure - Main npm test",
                "exitcode '1': failure",
            ],
        );
    }

    #[test]
    fn bitbucket_failed_step_flags_test_and_build_failures() {
        let found = error_texts(
            &BITBUCKET,
            concat!(
                "+ npm ci\n",
                "added 214 packages\n",
                "+ python manage.py test\n",
                "Traceback (most recent call last):\n",
                "  File \"manage.py\", line 12, in <module>\n",
                "ValueError: invalid literal for int() with base 10: 'x'\n",
                "+ make build\n",
                "make: *** [Makefile:12: build] Error 2\n",
                "Searching for test report files in directories named [test-results]\n",
            ),
        );

        assert_errors(
            &found,
            &[
                "Traceback (most recent call last):",
                "ValueError: invalid literal for int() with base 10: 'x'",
                "make: *** [Makefile:12: build] Error 2",
            ],
        );
    }

    #[test]
    fn tekton_failed_step_flags_container_output_failures() {
        let found = error_texts(
            &TEKTON,
            concat!(
                "\x1e2026-10-28T10:01:01Z Step run-tests\n",
                "2026-10-28T10:01:20Z running 3 tests\n",
                "2026-10-28T10:01:30Z test login::rejects_bad_password ... FAILED\n",
                "2026-10-28T10:01:30Z test result: FAILED. 2 passed; 1 failed\n",
                "2026-10-28T10:01:31Z {\"level\":\"error\",\"msg\":\"tests failed\"}\n",
                "2026-10-28T10:01:31Z time=\"2026-10-28T10:01:31Z\" level=fatal msg=\"exit status 1\"\n",
            ),
        );

        assert_errors(
            &found,
            &[
                "test login::rejects_bad_password ... FAILED",
                "test result: FAILED. 2 passed; 1 failed",
                "{\"level\":\"error\",\"msg\":\"tests failed\"}",
                "time=\"2026-10-28T10:01:31Z\" level=fatal msg=\"exit status 1\"",
            ],
        );
    }

    #[test]
    fn jenkins_unstable_build_is_an_error_like_its_failed_status() {
        let found = error_texts(
            &JENKINS,
            concat!(
                "Started by user admin\n",
                "[app] $ /bin/sh -xe /tmp/jenkins123.sh\n",
                "+ make test\n",
                "Tests run: 4, Failures: 1\n",
                "Finished: UNSTABLE\n",
            ),
        );

        assert_errors(&found, &["Finished: UNSTABLE"]);
    }

    #[test]
    fn providers_with_native_error_markers_do_not_infer_more() {
        let jenkins = error_texts(
            &JENKINS,
            concat!(
                "[Pipeline] sh\n",
                "+ make test\n",
                "test result: FAILED. 0 passed; 1 failed\n",
                "[Pipeline] }\n",
                "[Pipeline] // stage\n",
                "[Pipeline] error\n",
                "ERROR: script returned exit code 2\n",
                "Finished: FAILURE\n",
            ),
        );
        assert_errors(
            &jenkins,
            &["ERROR: script returned exit code 2", "Finished: FAILURE"],
        );

        let azure = error_texts(
            &AZURE,
            concat!(
                "2026-10-28T10:00:00.0000000Z ##[section]Starting: Bash\n",
                "2026-10-28T10:00:01.0000000Z npm ERR! Test failed.\n",
                "2026-10-28T10:00:02.0000000Z ##[error]Bash exited with code '1'.\n",
                "2026-10-28T10:00:02.0000000Z ##[section]Finishing: Bash\n",
            ),
        );
        assert_errors(&azure, &["Bash exited with code '1'."]);
    }

    #[test]
    fn ordinary_output_is_not_flagged_as_an_error() {
        let found = error_texts(
            &PLAIN,
            concat!(
                "0 errors\n",
                "error_handler.rs\n",
                "cargo test --fail-fast\n",
                "errors: none\n",
                "test errors::parse_error ... ok\n",
                "PASS src/error.test.ts\n",
                "✓ handles error responses\n",
                "Compiling error-chain v0.12.0\n",
                "ERROR_COUNT=0\n",
                "No errors found\n",
                "0 FAILED\n",
                "Process exited with code 0\n",
                "exit status 0\n",
                "log_level=error\n",
                "Error handling guide\n",
                "level=info msg=\"retrying after error\"\n",
                "Finished in 0.5s (0 failures)\n",
                "exit code 2 is documented below\n",
            ),
        );

        assert_errors(&found, &[]);
    }
}
