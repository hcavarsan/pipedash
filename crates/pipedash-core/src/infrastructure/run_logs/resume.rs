use std::collections::HashSet;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use chrono::{
    DateTime,
    SecondsFormat,
};

use super::checkpoint::{
    CHECKPOINT_FILE,
    TAIL_FILE,
};
use super::testing::{
    cache,
    doc_key,
    feed,
    finish,
    ingester,
};
use super::{
    BlockLimits,
    DocKey,
    Document,
    IngestOutcome,
    Ingested,
    Ingester,
    LogStore,
    PublishOutcome,
    StepBound,
    LINE_UNITS_MAX,
    MARKER_LINES_CAP,
    MAX_LINE_BYTES,
    PAGE_MAX_LINES,
};
use crate::domain::{
    LogCursor,
    LogLine,
    LogRead,
    PipelineStatus,
};

const BASE: i64 = 1_700_000_000_000;

struct Snapshot {
    lines: Vec<LogLine>,
    summary: serde_json::Map<String, serde_json::Value>,
}

fn snapshot(doc: &Document) -> Snapshot {
    let cache = cache();
    let meta = doc.meta();
    let mut lines = Vec::new();
    while lines.len() < meta.line_count as usize {
        let page = doc
            .read_lines(&cache, lines.len() as u32, PAGE_MAX_LINES, LINE_UNITS_MAX)
            .expect("page");
        assert!(!page.lines.is_empty(), "the document ends early");
        lines.extend(page.lines);
    }
    let serde_json::Value::Object(summary) = serde_json::json!({
        "complete": meta.complete,
        "truncated": meta.truncated,
        "line_count": meta.line_count,
        "text_bytes": meta.text_bytes,
        "first_timestamp_ms": meta.first_timestamp_ms,
        "styles": meta.styles,
        "sections": meta.sections,
        "error_lines": meta.error_lines,
        "warning_lines": meta.warning_lines,
        "error_total": meta.error_total,
        "warning_total": meta.warning_total,
        "focus_line": meta.focus_line,
    }) else {
        unreachable!("a JSON object literal");
    };

    Snapshot { lines, summary }
}

fn assert_same(left: &Snapshot, right: &Snapshot, what: &str) {
    for (key, value) in &left.summary {
        assert!(
            right.summary.get(key) == Some(value),
            "{what}: {key} differs"
        );
    }
    assert_eq!(left.lines.len(), right.lines.len(), "{what}: line count");
    if let Some(index) = (0..left.lines.len()).find(|&i| left.lines[i] != right.lines[i]) {
        let short = |line: &LogLine| line.text.chars().take(80).collect::<String>();
        panic!(
            "{what}: line {index} differs: {:?} vs {:?}",
            short(&left.lines[index]),
            short(&right.lines[index])
        );
    }
}

fn stamp(ms: i64) -> String {
    DateTime::from_timestamp_millis(BASE + ms)
        .expect("timestamp")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn step(id: &str, status: PipelineStatus, start: i64, end: i64) -> StepBound {
    StepBound {
        node_id: id.into(),
        title: id.into(),
        status,
        started_ms: Some(BASE + start * 1000),
        finished_ms: Some(BASE + end * 1000),
    }
}

fn steps() -> Vec<StepBound> {
    vec![
        step("setup", PipelineStatus::Success, 0, 19),
        step("build", PipelineStatus::Failed, 20, 24),
        step("post", PipelineStatus::Success, 25, 26),
    ]
}

fn corpus() -> Vec<u8> {
    let mut log = String::new();
    let mut line = |ms: i64, body: &str| log.push_str(&format!("{} {body}\n", stamp(ms)));
    line(0, "Current runner version: '2.300'");
    line(1, "##[group]Run setup");
    line(
        2,
        "\x1b[1;32mgreen bold\x1b[0m and \x1b[31mred\x1b[0m then \x1b[4;38;5;208mlink\x1b[0m",
    );
    line(3, "##[endgroup]");
    line(4, "##[group]Outer");
    line(5, "##[group]Inner");
    line(6, "inner text");
    line(7, "##[endgroup]");
    line(8, "outer text");
    line(9, "##[endgroup]");
    line(10, "##[warning]careful now");
    line(11, "[command]make all");
    line(12, "##[group]Never closed");
    line(13, "still inside");
    line(14, "progress 10%\rprogress 50%\rprogress 100%\r");
    line(15, "overwritten\r\r");
    line(16, "\u{e9}\u{4e2d}\u{1f600} multibyte");
    for i in 0..10_100 {
        let ms = 1000 + i;
        line(ms, &format!("##[error]boom {i}"));
    }
    line(20_000, "##[group]Run build");
    line(20_001, "compiling \x1b[33mwarn\x1b[0m");
    line(21_000, "##[error]link failed");
    line(21_001, "##[error]link failed again");
    line(22_000, "##[endgroup]");
    line(25_000, "Post job cleanup.");
    log.push_str("trailing \x1b[33myellow");

    log.into_bytes()
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn cuts(&mut self, len: usize, count: usize) -> Vec<usize> {
        let mut cuts: Vec<usize> = (0..count)
            .map(|_| (self.next() % (len as u64 + 1)) as usize)
            .collect();
        cuts.sort_unstable();
        cuts
    }
}

struct Lineage {
    _dir: tempfile::TempDir,
    store: Arc<LogStore>,
    key: DocKey,
    steps: Vec<StepBound>,
    limits: BlockLimits,
}

struct Extended {
    resumed: bool,
    outcome: IngestOutcome,
}

async fn feed_blocking(ingester: Ingester, data: Vec<u8>, chunk: usize) -> Ingested {
    tokio::task::spawn_blocking(move || feed(ingester, &data, chunk))
        .await
        .expect("ingest")
}

impl Lineage {
    async fn new(steps: Vec<StepBound>) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LogStore::open(dir.path().join("run-logs"), u64::MAX)
            .await
            .expect("store");

        Self {
            _dir: dir,
            store,
            key: doc_key(),
            steps,
            limits: BlockLimits::default(),
        }
    }

    async fn ingester(&self, dir: &std::path::Path, revision: i64, complete: bool) -> Ingester {
        let mut ingester = ingester(
            dir,
            self.key.clone(),
            revision,
            self.steps.clone(),
            self.limits.clone(),
        );
        ingester.complete = complete;
        ingester.base = self.store.base(&self.key).await.expect("base");

        ingester
    }

    async fn extend(
        &self, data: &[u8], complete: bool, cursor: Option<LogCursor>, chunk: usize, restart: bool,
    ) -> Extended {
        let tmp = self.store.tmp_dir().await.expect("tmp dir");
        let revision = self.store.allocate_revision();
        let epoch = self.store.epoch(self.key.provider_id);
        let mut ingester = self.ingester(&tmp, revision, complete).await;
        ingester.restart = Arc::new(AtomicBool::new(restart));
        let expected = ingester.base.as_ref().map(|base| base.revision);
        let ingested = feed_blocking(ingester, data.to_vec(), chunk).await;
        let resumed = ingested.resumed();
        let outcome = tokio::task::spawn_blocking(move || finish(ingested, cursor))
            .await
            .expect("finish");
        let published = self
            .store
            .publish(&self.key, &tmp, revision, epoch, expected, HashSet::new())
            .await
            .expect("publish");
        assert_eq!(published, PublishOutcome::Published);

        Extended { resumed, outcome }
    }

    async fn document(&self) -> (Arc<Document>, super::ReadLease) {
        let revision = self.store.current(&self.key).expect("current revision");
        self.store
            .lease(&self.key, revision)
            .await
            .expect("lease")
            .expect("published revision")
    }

    async fn latest(&self) -> Snapshot {
        snapshot(&self.document().await.0)
    }
}

async fn fresh(data: &[u8], complete: bool, steps: &[StepBound], chunk: usize) -> Snapshot {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut ingester = ingester(
        dir.path(),
        doc_key(),
        1,
        steps.to_vec(),
        BlockLimits::default(),
    );
    ingester.complete = complete;
    finish(feed_blocking(ingester, data.to_vec(), chunk).await, None);

    snapshot(&Document::open(dir.path()).expect("open"))
}

fn cursor(position: usize) -> Option<LogCursor> {
    Some(LogCursor::new(&position).expect("cursor"))
}

async fn assert_every_increment_matches_a_one_shot(
    log: &[u8], cuts: &[usize], chunk: usize, what: &str,
) {
    let lineage = Lineage::new(steps()).await;
    let ends: Vec<usize> = cuts.iter().copied().chain([log.len()]).collect();
    let mut start = 0;
    for (index, &end) in ends.iter().enumerate() {
        let last = index + 1 == ends.len();
        let extended = lineage
            .extend(&log[start..end], last, cursor(index), chunk, false)
            .await;
        let label = format!("{what}, increment {index} of {cuts:?}");

        assert_eq!(extended.resumed, index > 0, "{label}");
        assert_eq!(extended.outcome.resumable, !last, "{label}");
        assert_same(
            &lineage.latest().await,
            &fresh(&log[..end], last, &steps(), chunk).await,
            &label,
        );
        start = end;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn resuming_matches_a_one_shot_ingest_for_any_split() {
    let log = corpus();
    let whole = fresh(&log, true, &steps(), 4096).await;
    assert!(whole.summary["error_total"].as_u64().unwrap() > MARKER_LINES_CAP as u64);
    assert!(whole.summary["error_lines"].as_array().unwrap().len() == MARKER_LINES_CAP);
    assert!(whole.summary["focus_line"].as_u64().unwrap() > MARKER_LINES_CAP as u64);
    assert!(whole.lines.last().unwrap().text.starts_with("trailing "));

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..16 {
        let parts = 1 + (rng.next() % 6) as usize;
        let cuts = rng.cuts(log.len(), parts);
        let chunk = [97, 4096, 65_536][round % 3];
        assert_every_increment_matches_a_one_shot(&log, &cuts, chunk, &format!("round {round}"))
            .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_split_at_every_byte_of_tricky_input_matches_a_one_shot_ingest() {
    let log = format!(
        concat!(
            "{a} ##[group]Run it\r\n",
            "{b} \x1b[31;1mred\x1b[0m \x1b[38;2;1;2;3mtrue\x1b[0m\r\n",
            "{b} half\rwhole\r\r\n",
            "{b} \u{e9}\u{4e2d}\u{1f600}\n",
            "{b} ##[error]bad\n",
            "{b} ##[endgroup]\n",
            "{b} ##[group]open at the end\n",
            "{b} last \x1b[1"
        ),
        a = stamp(0),
        b = stamp(1)
    )
    .into_bytes();

    let lineage_steps = steps();
    for cut in 0..=log.len() {
        let lineage = Lineage::new(lineage_steps.clone()).await;
        lineage
            .extend(&log[..cut], false, cursor(0), 3, false)
            .await;
        lineage.extend(&log[cut..], true, cursor(1), 5, false).await;

        assert_same(
            &lineage.latest().await,
            &fresh(&log, true, &lineage_steps, 4096).await,
            &format!("cut at byte {cut}"),
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn giant_lines_and_unterminated_tails_split_like_a_one_shot_ingest() {
    let mut log = format!("{} start\n", stamp(0)).into_bytes();
    let head = log.len();
    log.extend_from_slice("\u{e9}".repeat(MAX_LINE_BYTES + 500).as_bytes());
    let giant_end = log.len();
    log.extend_from_slice(b"\nmiddle\n");
    log.extend_from_slice(&b"x".repeat(MAX_LINE_BYTES + 10));
    let len = log.len();

    for cuts in [
        vec![head],
        vec![head + MAX_LINE_BYTES - 1],
        vec![head + MAX_LINE_BYTES],
        vec![head + MAX_LINE_BYTES + 1, giant_end + 3],
        vec![giant_end - 1, giant_end],
        vec![head + 1, head + 2 * MAX_LINE_BYTES + 3, len - 4],
    ] {
        assert_every_increment_matches_a_one_shot(&log, &cuts, 64 * 1024, "giant").await;
    }
}

#[tokio::test]
async fn increments_chain_through_empty_reads() {
    let lineage = Lineage::new(Vec::new()).await;
    lineage.extend(b"one\ntw", false, cursor(0), 4, false).await;
    lineage.extend(b"", false, cursor(1), 4, false).await;
    lineage
        .extend(b"o\nthree", false, cursor(2), 4, false)
        .await;
    lineage.extend(b"", true, cursor(3), 4, false).await;

    assert_same(
        &lineage.latest().await,
        &fresh(b"one\ntwo\nthree", true, &[], 4).await,
        "empty increments",
    );
}

#[tokio::test]
async fn appended_revisions_report_the_lines_they_share_with_the_previous_one() {
    let lineage = Lineage::new(Vec::new()).await;

    lineage
        .extend(b"a\nb\nc", false, cursor(0), 64, false)
        .await;
    let (first, _lease) = lineage.document().await;
    let first = first.meta().clone();
    assert_eq!(first.line_count, 3);
    assert_eq!((first.stable_lines, first.previous_revision), (0, None));
    assert!(first.updated_at_ms > 0);

    lineage
        .extend(b"\nd\ne\n", false, cursor(1), 64, false)
        .await;
    let (second, _lease) = lineage.document().await;
    let second = second.meta().clone();
    assert_eq!(second.line_count, 5);
    assert_eq!(second.stable_lines, 2);
    assert_eq!(second.previous_revision, Some(first.revision));
    assert!(second.updated_at_ms >= first.updated_at_ms);
}

#[tokio::test]
async fn a_restart_ignores_the_stored_log() {
    let lineage = Lineage::new(Vec::new()).await;
    lineage
        .extend(b"old one\nold two\n", false, cursor(0), 64, false)
        .await;

    let extended = lineage.extend(b"new\n", false, cursor(1), 64, true).await;

    assert!(!extended.resumed);
    let (document, _lease) = lineage.document().await;
    assert_eq!(document.meta().line_count, 1);
    assert_eq!(
        (
            document.meta().stable_lines,
            document.meta().previous_revision
        ),
        (0, None)
    );
    assert_same(
        &lineage.latest().await,
        &fresh(b"new\n", false, &[], 64).await,
        "after a restart",
    );
}

#[tokio::test]
async fn truncated_documents_never_resume() {
    let mut lineage = Lineage::new(Vec::new()).await;
    lineage.limits.max_doc_text_bytes = 20;

    let extended = lineage
        .extend(&b"0123456789\n".repeat(5), false, cursor(0), 8, false)
        .await;

    assert!(extended.outcome.truncated && !extended.outcome.resumable);
    let base = lineage.store.base(&lineage.key).await.unwrap().unwrap();
    assert!(base.checkpoint.is_none());
    assert!(base.cursor(&lineage.steps).is_none());
    assert!(!base.document.dir().join(CHECKPOINT_FILE).exists());
}

#[tokio::test]
async fn complete_revisions_carry_no_checkpoint() {
    let lineage = Lineage::new(Vec::new()).await;
    lineage
        .extend(b"##[error]x\nrest", false, cursor(0), 64, false)
        .await;
    let (document, lease) = lineage.document().await;
    assert!(document.dir().join(CHECKPOINT_FILE).exists());
    assert!(document.dir().join(TAIL_FILE).exists());
    assert!(document.dir().join("errors.bin").exists());
    drop(lease);

    lineage.extend(b"", true, cursor(1), 64, false).await;
    let (document, _lease) = lineage.document().await;

    for name in [CHECKPOINT_FILE, TAIL_FILE, "errors.bin"] {
        assert!(!document.dir().join(name).exists(), "{name} was kept");
    }
    let base = lineage.store.base(&lineage.key).await.unwrap().unwrap();
    assert!(base.cursor(&lineage.steps).is_none());
}

#[tokio::test]
async fn a_damaged_checkpoint_falls_back_to_a_fresh_read() {
    for damage in ["checkpoint", "blocks"] {
        let lineage = Lineage::new(Vec::new()).await;
        lineage.extend(b"a\nb\n", false, cursor(0), 64, false).await;
        let base = lineage.store.base(&lineage.key).await.unwrap().unwrap();
        assert!(base.cursor(&lineage.steps).is_some());
        let dir = base.document.dir().to_path_buf();
        match damage {
            "checkpoint" => fs::write(dir.join(CHECKPOINT_FILE), b"{").unwrap(),
            _ => fs::write(dir.join("blocks.bin"), b"").unwrap(),
        }
        drop(base);

        let base = lineage.store.base(&lineage.key).await.unwrap().unwrap();
        assert!(base.cursor(&lineage.steps).is_none(), "{damage}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn changed_step_bounds_before_the_checkpoint_forbid_resuming() {
    let lineage = Lineage::new(steps()).await;
    let log = corpus();
    lineage
        .extend(&log[..log.len() / 2], false, cursor(0), 4096, false)
        .await;
    let base = lineage.store.base(&lineage.key).await.unwrap().unwrap();

    assert!(base.cursor(&steps()).is_some());
    let mut shifted = steps();
    shifted[0].started_ms = Some(BASE - 5000);
    shifted[0].node_id = "other".into();
    assert!(base.cursor(&shifted).is_none());
    let mut later = steps();
    later.truncate(1);
    later[0].finished_ms = None;
    later[0].status = PipelineStatus::Running;
    assert!(base.cursor(&later).is_some());
}

#[tokio::test]
async fn appends_with_nothing_new_are_recognised_as_unchanged() {
    let lineage = Lineage::new(Vec::new()).await;
    lineage.extend(b"a\nb", false, cursor(7), 64, false).await;
    let tmp = lineage.store.tmp_dir().await.unwrap();
    let same = LogRead::append(LogCursor::new(&7).unwrap());
    let moved = LogRead::append(LogCursor::new(&8).unwrap());

    let idle = feed_blocking(lineage.ingester(&tmp, 99, false).await, Vec::new(), 8).await;
    assert!(idle.resumed());
    assert!(idle.unchanged(&same));
    assert!(!idle.unchanged(&moved));

    let busy = feed_blocking(lineage.ingester(&tmp, 99, false).await, b"c\n".to_vec(), 8).await;
    assert!(!busy.unchanged(&same));

    let done = feed_blocking(lineage.ingester(&tmp, 99, true).await, Vec::new(), 8).await;
    assert!(!done.unchanged(&same));
}

#[tokio::test]
async fn identical_snapshots_are_recognised_by_their_content() {
    let lineage = Lineage::new(Vec::new()).await;
    lineage.extend(b"x\ny\n", false, None, 64, false).await;
    let tmp = lineage.store.tmp_dir().await.unwrap();
    let read = LogRead::replace(None);

    let same = feed_blocking(
        lineage.ingester(&tmp, 99, false).await,
        b"x\ny\n".to_vec(),
        3,
    )
    .await;
    assert!(!same.resumed());
    assert!(same.unchanged(&read));

    let other = feed_blocking(
        lineage.ingester(&tmp, 99, false).await,
        b"x\nz\n".to_vec(),
        3,
    )
    .await;
    assert!(!other.unchanged(&read));

    let done = feed_blocking(
        lineage.ingester(&tmp, 99, true).await,
        b"x\ny\n".to_vec(),
        3,
    )
    .await;
    assert!(!done.unchanged(&read));
}
