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
        let kind = kind_of(parsed.marker);
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

#[cfg(test)]
mod tests {
    use pipedash_plugin_buildkite::BuildkiteLogDialect;
    use pipedash_plugin_gitlab::GitlabLogDialect;

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

        let meta = ingester.run(&mut rx).expect("ingest").meta;

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

        let meta = ingester.run(&mut rx).expect("ingest").meta;

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
}
