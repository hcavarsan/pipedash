use chrono::Utc;
use futures::StreamExt;

use super::*;
use crate::application::test_support::{
    pipeline,
    stub_run,
    Gate,
    Harness,
    StubLogMode,
    StubProvider,
};
use crate::domain::{
    LogCursor,
    LogDocumentMeta,
    PipelineStatus,
};
use crate::infrastructure::run_logs::testing as logs;
use crate::infrastructure::run_logs::SEARCH_MAX_QUERY_CHARS;

const RUN: i64 = 7;

fn log_node(
    id: &str, parent: Option<&str>, kind: LogNodeKind, status: PipelineStatus,
    log: Option<LogAvailability>,
) -> LogNode {
    LogNode {
        id: id.to_string(),
        parent_id: parent.map(str::to_string),
        kind,
        name: format!("node {id}"),
        status,
        started_at: Some(Utc::now()),
        finished_at: Some(Utc::now()),
        log,
        web_url: None,
    }
}

fn job(id: &str, status: PipelineStatus, log: LogAvailability) -> LogNode {
    log_node(id, None, LogNodeKind::Job, status, Some(log))
}

fn step(id: &str, parent: &str) -> LogNode {
    log_node(
        id,
        Some(parent),
        LogNodeKind::Step,
        PipelineStatus::Success,
        None,
    )
}

fn default_nodes() -> Vec<LogNode> {
    vec![
        job("1", PipelineStatus::Success, LogAvailability::Ready),
        step("1:1", "1"),
        job("2", PipelineStatus::Success, LogAvailability::Ready),
    ]
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}"
        );
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn meta_of(document: RunLogDocument) -> LogDocumentMeta {
    match document {
        RunLogDocument::Ready(meta) => meta,
        other => panic!("expected a ready document, got {other:?}"),
    }
}

struct Fixture {
    h: Harness,
    id: i64,
    pipeline_id: String,
    stub: Arc<StubProvider>,
    live: bool,
}

impl Fixture {
    async fn new(build: impl FnOnce(StubProvider) -> StubProvider) -> Self {
        Self::with_logs(default_nodes(), &["line one\nline two\n"], build).await
    }

    async fn with_logs(
        nodes: Vec<LogNode>, chunks: &[&str], build: impl FnOnce(StubProvider) -> StubProvider,
    ) -> Self {
        let h = Harness::new().await;
        let (id, stub) = h
            .add_stub(|id| {
                let pipeline_id = format!("github__{id}__web");
                build(
                    StubProvider::new(id)
                        .with_logs(nodes, chunks)
                        .with_runs(vec![stub_run(&pipeline_id, RUN, 60)]),
                )
            })
            .await;
        h.repository
            .update_pipelines_cache(id, &[pipeline(id, "web", PipelineStatus::Success)])
            .await
            .unwrap();

        Self {
            h,
            id,
            pipeline_id: format!("github__{id}__web"),
            stub,
            live: false,
        }
    }

    fn svc(&self) -> &Arc<LogService> {
        &self.h.log_service
    }

    fn key(&self, node_id: &str) -> DocKey {
        DocKey {
            provider_id: self.id,
            pipeline_id: self.pipeline_id.clone(),
            run_number: RUN,
            node_id: node_id.to_string(),
        }
    }

    async fn open(&self, node_id: &str) -> RunLogDocument {
        self.open_with(node_id, false).await
    }

    async fn open_with(&self, node_id: &str, retry: bool) -> RunLogDocument {
        self.svc()
            .open_document(&self.pipeline_id, RUN, node_id, retry)
            .await
            .unwrap()
    }

    async fn ready(&self, count: usize) {
        until("a ready event", || {
            self.h.bus.count("run-log-ready") >= count
        })
        .await;
    }

    async fn failed(&self, count: usize) {
        until("a failed event", || {
            self.h.bus.count("run-log-failed") >= count
        })
        .await;
    }

    async fn ready_meta(&self, node_id: &str) -> LogDocumentMeta {
        assert!(matches!(
            self.open(node_id).await,
            RunLogDocument::Loading { .. }
        ));
        self.ready(self.h.bus.count("run-log-ready") + 1).await;
        let meta = meta_of(self.open(node_id).await);
        if self.live {
            self.settle().await;
        }

        meta
    }

    fn tmp_entries(&self) -> usize {
        std::fs::read_dir(self.h._temp_dir.path().join("run-logs/v1/tmp"))
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    async fn warm(&self) {
        self.svc().provider_of(&self.pipeline_id).await.unwrap();
        self.h.provider_service.acquire(self.id).await.unwrap();
    }

    async fn live_with(
        nodes: Vec<LogNode>, chunks: &[&str], build: impl FnOnce(StubProvider) -> StubProvider,
    ) -> Self {
        let mut f = Self::with_logs(nodes, chunks, build).await;
        f.live = true;
        f.stub.runs.lock().unwrap()[0].status = PipelineStatus::Running;
        f.warm().await;
        tokio::time::pause();
        f
    }

    async fn live(chunks: &[&str], build: impl FnOnce(StubProvider) -> StubProvider) -> Self {
        Self::live_with(partial_nodes(), chunks, build).await
    }

    async fn settle(&self) {
        for _ in 0..50 {
            tokio::task::yield_now().await;
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    async fn tick(&self, seconds: u64) {
        tokio::time::advance(Duration::from_secs(seconds) + Duration::from_millis(2)).await;
        self.settle().await;
    }

    async fn texts(&self, meta: &LogDocumentMeta) -> Vec<String> {
        self.svc()
            .read_lines(
                &self.pipeline_id,
                RUN,
                &meta.node_id,
                meta.revision,
                0,
                100,
                4096,
            )
            .await
            .unwrap()
            .lines
            .into_iter()
            .map(|line| line.text)
            .collect()
    }
}

fn partial_nodes() -> Vec<LogNode> {
    vec![job("1", PipelineStatus::Running, LogAvailability::Partial)]
}

fn resumable(stub: StubProvider) -> StubProvider {
    stub.with_log_mode(StubLogMode::Resumable)
}

fn gated(gate: &Arc<Gate>) -> impl FnOnce(StubProvider) -> StubProvider + '_ {
    move |stub| stub.log_gated(gate)
}

#[tokio::test]
async fn open_downloads_once_then_serves_the_cached_document() {
    let f = Fixture::new(|stub| stub).await;

    assert!(matches!(
        f.open("1").await,
        RunLogDocument::Loading { received_bytes: 0 }
    ));
    f.ready(1).await;
    let meta = meta_of(f.open("1").await);

    assert_eq!((meta.line_count, meta.complete), (2, true));
    assert_eq!(f.stub.log_read_count(), 1);
    let ready =
        f.h.bus
            .events()
            .into_iter()
            .find_map(|event| match event {
                CoreEvent::RunLogReady { revision, .. } => Some(revision),
                _ => None,
            })
            .unwrap();
    assert_eq!(ready, meta.revision);
    let page = f
        .svc()
        .read_lines(&f.pipeline_id, RUN, "1", meta.revision, 0, 10, 4096)
        .await
        .unwrap();
    let text: Vec<_> = page.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(text, ["line one", "line two"]);
}

#[tokio::test]
async fn concurrent_opens_share_one_read() {
    let gate = Gate::new();
    let f = Fixture::new(gated(&gate)).await;

    let (a, b) = tokio::join!(f.open("1"), f.open("1"));
    gate.arrived().await;

    assert!(matches!(a, RunLogDocument::Loading { .. }));
    assert!(matches!(b, RunLogDocument::Loading { .. }));
    assert!(matches!(f.open("1").await, RunLogDocument::Loading { .. }));
    gate.open();
    f.ready(1).await;
    assert_eq!(f.stub.log_read_count(), 1);
}

#[tokio::test]
async fn progress_ticks_never_restart_the_read() {
    let gate = Gate::new();
    let f = Fixture::new(gated(&gate)).await;
    f.warm().await;
    tokio::time::pause();

    f.open("1").await;
    until("the read", || f.stub.log_read_count() == 1).await;
    for _ in 0..10 {
        tokio::time::advance(Duration::from_millis(500)).await;
        tokio::task::yield_now().await;
    }

    assert!(matches!(f.open("1").await, RunLogDocument::Loading { .. }));
    assert_eq!(f.stub.log_read_count(), 1);
    gate.open();
    f.ready(1).await;
    assert_eq!(f.stub.log_read_count(), 1);
}

#[tokio::test]
async fn clear_during_an_ingest_publishes_nothing() {
    let gate = Gate::new();
    let f = Fixture::new(gated(&gate)).await;

    f.open("1").await;
    gate.arrived().await;
    f.svc().clear().await.unwrap();
    gate.open();
    until("the ingest to stop", || f.svc().inflight.is_empty()).await;

    assert_eq!(f.tmp_entries(), 0);
    assert!(f.h.log_store.current(&f.key("1")).is_none());
    assert!(f
        .h
        .bus
        .names()
        .iter()
        .all(|name| !name.starts_with("run-log")));
}

#[tokio::test]
async fn clear_during_a_tree_fetch_neither_saves_nor_memoizes_the_tree() {
    let gate = Gate::new();
    let f = Fixture::new(|stub| stub.log_nodes_gated(&gate)).await;

    let svc = Arc::clone(f.svc());
    let pipeline_id = f.pipeline_id.clone();
    let fetch = tokio::spawn(async move { svc.run_log_tree(&pipeline_id, RUN).await });
    gate.arrived().await;
    f.svc().clear().await.unwrap();
    gate.open();
    fetch.await.unwrap().unwrap();

    assert!(f
        .h
        .log_store
        .read_tree(f.id, &f.pipeline_id, RUN)
        .await
        .is_none());
    assert!(f.svc().tree_memo.is_empty());
    f.svc().run_log_tree(&f.pipeline_id, RUN).await.unwrap();
    assert_eq!(f.stub.log_node_fetch_count(), 2);
}

#[tokio::test]
async fn shutdown_stops_queued_ingests_searches_and_blocked_exports() {
    let big = "x\n".repeat(7000);
    let f = Fixture::with_logs(default_nodes(), &[big.as_str()], |stub| stub).await;
    let svc = Arc::clone(f.svc());

    let meta = f.ready_meta("1").await;
    let _ingests = Arc::clone(&svc.ingest_permits)
        .acquire_many_owned(INGEST_PERMITS as u32)
        .await
        .unwrap();
    assert!(matches!(f.open("2").await, RunLogDocument::Loading { .. }));
    let export = svc
        .prepare_export(&f.pipeline_id, RUN, "1", meta.revision)
        .await
        .unwrap();
    let stream = export.into_stream().unwrap();
    let _searches = Arc::clone(&svc.search_permits)
        .acquire_many_owned(SEARCH_PERMITS as u32)
        .await
        .unwrap();
    let search = {
        let svc = Arc::clone(&svc);
        let pipeline_id = f.pipeline_id.clone();
        tokio::spawn(async move {
            let query = LogSearchQuery {
                text: "x".into(),
                regex: false,
                case_sensitive: false,
            };
            svc.search(&pipeline_id, RUN, "1", meta.revision, "s", &query)
                .await
        })
    };
    until("the search to queue", || svc.searches.len() == 1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    tokio::time::timeout(Duration::from_secs(10), svc.shutdown())
        .await
        .expect("shutdown hung");

    assert!(svc.tracker.is_empty());
    assert!(svc.inflight.is_empty());
    assert!(search.await.unwrap().is_err());
    assert!(svc.admit(|_| {}).is_err());
    let items: Vec<io::Result<Bytes>> = stream.collect().await;
    assert!(items.last().is_some_and(|item| item.is_err()));
}

#[tokio::test]
async fn an_old_core_shutting_down_leaves_the_new_one_working() {
    let gate = Gate::new();
    let f = Fixture::new(gated(&gate)).await;
    let root = f.h._temp_dir.path().join("run-logs");
    let store = LogStore::open(root, u64::MAX).await.unwrap();
    assert!(Arc::ptr_eq(&store, &f.h.log_store));
    let new = Arc::new(LogService::new(
        store,
        Arc::clone(&f.h.provider_service),
        Arc::clone(&f.h.repository),
        f.h.bus.clone(),
    ));

    f.open("1").await;
    gate.arrived().await;
    let old = Arc::clone(f.svc());
    let shutdown = tokio::spawn(async move { old.shutdown().await });
    until("the old core to close", || f.svc().admit(|_| {}).is_err()).await;
    gate.open();
    shutdown.await.unwrap();

    assert_eq!(f.tmp_entries(), 0);
    assert!(matches!(
        new.open_document(&f.pipeline_id, RUN, "1", false)
            .await
            .unwrap(),
        RunLogDocument::Loading { .. }
    ));
    f.ready(1).await;
    assert!(matches!(
        new.open_document(&f.pipeline_id, RUN, "1", false)
            .await
            .unwrap(),
        RunLogDocument::Ready(_)
    ));
}

#[tokio::test]
async fn updating_the_provider_mid_download_fails_once_and_the_next_open_starts_fresh() {
    let gate = Gate::new();
    let f = Fixture::new(gated(&gate)).await;

    f.open("1").await;
    gate.arrived().await;
    let mut config = f.h.config("renamed", 45);
    config.id = Some(f.id);
    f.h.provider_service
        .update_provider(f.id, config)
        .await
        .unwrap();
    gate.open();
    f.failed(1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(f.h.bus.count("run-log-failed"), 1);
    assert!(f.h.bus.events().iter().any(|event| matches!(
        event,
        CoreEvent::RunLogFailed { message, .. } if message == PROVIDER_CHANGED
    )));
    until("the old attempt to leave", || f.svc().inflight.is_empty()).await;
    assert!(matches!(f.open("1").await, RunLogDocument::Loading { .. }));
    f.ready(1).await;
    assert_eq!(f.stub.log_read_count(), 2);
}

#[tokio::test]
async fn removing_the_provider_fails_once_and_drops_its_cache() {
    let gate = Gate::new();
    let f = Fixture::new(gated(&gate)).await;
    gate.open();
    f.ready_meta("1").await;
    let provider_dir = f.h._temp_dir.path().join(format!("run-logs/v1/{}", f.id));
    assert!(provider_dir.exists());

    gate.close();
    f.open("2").await;
    gate.arrived().await;
    f.h.provider_service.remove_provider(f.id).await.unwrap();
    gate.open();
    f.failed(1).await;
    until("the attempt to leave", || f.svc().inflight.is_empty()).await;

    assert_eq!(f.h.bus.count("run-log-failed"), 1);
    assert!(!provider_dir.exists());
    let err = f
        .svc()
        .open_document(&f.pipeline_id, RUN, "1", false)
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::PipelineNotFound(_)));
}

#[tokio::test]
async fn pending_nodes_never_read() {
    let nodes = vec![job("1", PipelineStatus::Running, LogAvailability::Pending)];
    let f = Fixture::with_logs(nodes, &["x\n"], |stub| stub).await;

    assert!(matches!(f.open("1").await, RunLogDocument::Pending));
    assert_eq!(f.stub.log_read_count(), 0);
}

#[tokio::test]
async fn a_live_log_resumes_from_its_cursor_and_publishes_only_the_new_lines() {
    let f = Fixture::live(&["one\ntwo\n"], resumable).await;
    let first = f.ready_meta("1").await;
    for _ in 0..5 {
        f.open("1").await;
    }
    assert_eq!(f.stub.log_read_count(), 1);

    f.stub.append_log("three\n");
    f.tick(2).await;
    f.ready(2).await;
    let second = meta_of(f.open("1").await);

    assert_eq!(
        (
            first.line_count,
            first.stable_lines,
            first.previous_revision
        ),
        (2, 0, None)
    );
    assert_eq!((second.line_count, second.stable_lines), (3, 2));
    assert_eq!(second.previous_revision, Some(first.revision));
    assert!(!second.complete && second.updated_at_ms >= first.updated_at_ms);
    assert_eq!(f.texts(&second).await, ["one", "two", "three"]);
    let requests = f.stub.log_requests();
    assert!(requests[0].is_none() && requests[1].is_some());
    assert_eq!(requests.len(), 2);
}

#[tokio::test]
async fn a_read_with_nothing_new_publishes_no_revision_and_no_event() {
    let f = Fixture::live(&["one\n"], resumable).await;
    let first = f.ready_meta("1").await;

    f.tick(2).await;
    until("the idle read", || f.stub.log_read_count() == 2).await;
    f.settle().await;
    f.tick(2).await;
    until("the second idle read", || f.stub.log_read_count() == 3).await;
    f.settle().await;

    assert_eq!(f.h.bus.count("run-log-ready"), 1);
    assert_eq!(f.h.log_store.current(&f.key("1")), Some(first.revision));
    assert_eq!(f.stub.log_requests().iter().flatten().count(), 2);
}

#[tokio::test]
async fn snapshot_logs_are_polled_slowly_and_identical_snapshots_publish_nothing() {
    let f = Fixture::live(&["one\n"], |stub| stub).await;
    let first = f.ready_meta("1").await;

    f.tick(9).await;
    assert_eq!(f.stub.log_read_count(), 1);
    f.tick(1).await;
    until("the second snapshot", || f.stub.log_read_count() == 2).await;
    f.settle().await;
    assert_eq!(f.h.bus.count("run-log-ready"), 1);
    assert_eq!(f.h.log_store.current(&f.key("1")), Some(first.revision));

    f.stub.append_log("two\n");
    f.tick(10).await;
    f.ready(2).await;
    let second = meta_of(f.open("1").await);
    assert_eq!(f.texts(&second).await, ["one", "two"]);
    assert_eq!((second.stable_lines, second.previous_revision), (0, None));
}

#[tokio::test]
async fn a_provider_that_cannot_continue_restarts_the_stored_log() {
    let f = Fixture::live(&["old one\nold two\n"], resumable).await;
    f.ready_meta("1").await;

    f.stub.replace_log("fresh\n");
    f.tick(2).await;
    f.ready(2).await;
    let second = meta_of(f.open("1").await);

    assert_eq!(f.texts(&second).await, ["fresh"]);
    assert_eq!((second.stable_lines, second.previous_revision), (0, None));
    assert!(f.stub.log_requests()[1].is_some());
}

#[tokio::test]
async fn a_provider_that_breaks_the_read_contract_publishes_nothing() {
    let f = Fixture::with_logs(partial_nodes(), &["x\n"], |stub| {
        stub.with_log_mode(StubLogMode::Misbehaving)
    })
    .await;

    assert!(matches!(f.open("1").await, RunLogDocument::Loading { .. }));
    f.failed(1).await;
    until("the attempt to leave", || f.svc().inflight.is_empty()).await;

    assert!(f.h.log_store.current(&f.key("1")).is_none());
    assert_eq!(f.h.bus.count("run-log-ready"), 0);
    assert_eq!(f.tmp_entries(), 0);
    assert!(matches!(
        f.open("1").await,
        RunLogDocument::Unavailable { message } if message == PROVIDER_MISBEHAVED
    ));
}

#[tokio::test]
async fn live_reads_stop_when_nobody_keeps_the_log_open_and_start_again_when_somebody_does() {
    let f = Fixture::live(&["a\n"], resumable).await;
    f.ready_meta("1").await;
    f.tick(2).await;
    until("a second read", || f.stub.log_read_count() == 2).await;
    f.settle().await;

    f.tick(50).await;
    until("the live task to stop", || f.svc().lives.is_empty()).await;
    let reads = f.stub.log_read_count();
    f.tick(30).await;

    assert_eq!(f.stub.log_read_count(), reads);
    assert!(f.svc().interests.is_empty());
    f.open("1").await;
    assert_eq!(f.svc().lives.len(), 1);
    f.tick(2).await;
    until("the reads to start again", || {
        f.stub.log_read_count() > reads
    })
    .await;
}

#[tokio::test]
async fn a_failed_live_read_keeps_the_stored_log_and_backs_off() {
    let f = Fixture::live(&["a\n"], resumable).await;
    let first = f.ready_meta("1").await;

    *f.stub.log_failure.lock().unwrap() = Some(DomainError::NetworkError("down".into()));
    f.tick(2).await;
    f.failed(1).await;
    assert_eq!(meta_of(f.open("1").await).revision, first.revision);
    for _ in 0..9 {
        f.tick(30).await;
        f.open("1").await;
    }
    assert_eq!(f.stub.log_read_count(), 2);

    *f.stub.log_failure.lock().unwrap() = None;
    f.stub.append_log("b\n");
    for _ in 0..3 {
        f.tick(30).await;
        f.open("1").await;
    }
    f.ready(2).await;
    assert_eq!(meta_of(f.open("1").await).line_count, 2);
}

#[tokio::test]
async fn tree_and_run_changes_are_pushed_without_being_asked_for() {
    let f = Fixture::live(&["a\n"], resumable).await;
    f.ready_meta("1").await;

    f.tick(5).await;
    assert_eq!(f.h.bus.count("run-log-tree-changed"), 0);
    assert_eq!(f.h.bus.count("run-updated"), 0);

    f.stub.log_nodes.lock().unwrap()[0].finished_at = Some(Utc::now());
    f.tick(5).await;
    until("the tree event", || {
        f.h.bus.count("run-log-tree-changed") == 1
    })
    .await;
    assert_eq!(f.h.bus.count("run-updated"), 0);

    f.stub.runs.lock().unwrap()[0].duration_seconds = Some(61);
    f.tick(5).await;
    until("the run event", || f.h.bus.count("run-updated") == 1).await;
    assert_eq!(f.h.bus.count("run-log-tree-changed"), 1);
    let updated =
        f.h.bus
            .events()
            .into_iter()
            .find_map(|event| match event {
                CoreEvent::RunUpdated {
                    pipeline_id,
                    run_number,
                    run,
                } => Some((pipeline_id, run_number, run.duration_seconds)),
                _ => None,
            })
            .unwrap();
    assert_eq!(updated, (f.pipeline_id.clone(), RUN, Some(61)));

    f.tick(5).await;
    assert_eq!(f.h.bus.count("run-log-tree-changed"), 1);
    assert_eq!(f.h.bus.count("run-updated"), 1);
}

#[tokio::test]
async fn watching_only_the_tree_pushes_changes_without_reading_any_log() {
    let f = Fixture::live(&["a\n"], resumable).await;

    f.svc().run_log_tree(&f.pipeline_id, RUN).await.unwrap();
    assert_eq!(f.svc().lives.len(), 1);
    f.stub.log_nodes.lock().unwrap()[0].status = PipelineStatus::Failed;
    f.tick(5).await;

    until("the tree event", || {
        f.h.bus.count("run-log-tree-changed") == 1
    })
    .await;
    assert_eq!(f.stub.log_read_count(), 0);
}

#[tokio::test]
async fn complete_logs_and_trees_start_no_live_task() {
    let f = Fixture::new(|stub| stub).await;

    f.svc().run_log_tree(&f.pipeline_id, RUN).await.unwrap();
    f.ready_meta("1").await;
    f.open("1").await;

    assert!(f.svc().lives.is_empty());
    assert!(f.svc().interests.is_empty());
}

#[tokio::test]
async fn a_watched_pending_node_is_read_as_soon_as_it_has_a_log() {
    let nodes = vec![job("1", PipelineStatus::Running, LogAvailability::Pending)];
    let f = Fixture::live_with(nodes, &["a\n"], resumable).await;
    assert!(matches!(f.open("1").await, RunLogDocument::Pending));

    *f.stub.log_nodes.lock().unwrap() = partial_nodes();
    f.tick(5).await;

    f.ready(1).await;
    assert_eq!(f.stub.log_read_count(), 1);
}

#[tokio::test]
async fn a_node_that_turns_ready_gets_one_final_read_and_the_live_task_ends() {
    let f = Fixture::live(&["a\n"], resumable).await;
    let first = f.ready_meta("1").await;

    f.stub.append_log("b\nlast");
    *f.stub.log_nodes.lock().unwrap() =
        vec![job("1", PipelineStatus::Success, LogAvailability::Ready)];
    f.stub.runs.lock().unwrap()[0].status = PipelineStatus::Success;
    f.tick(5).await;
    f.ready(2).await;
    until("the live task to end", || f.svc().lives.is_empty()).await;

    let last = meta_of(f.open("1").await);
    assert!(last.complete);
    assert_eq!(f.texts(&last).await, ["a", "b", "last"]);
    assert_eq!(
        (last.stable_lines, last.previous_revision),
        (1, Some(first.revision))
    );
    assert!(f.stub.log_requests()[1].is_some());
    let reads = f.stub.log_read_count();
    f.tick(60).await;
    assert_eq!(f.stub.log_read_count(), reads);
    assert!(f.svc().interests.is_empty());
}

#[tokio::test]
async fn truncated_logs_are_never_refreshed() {
    let f = Fixture::live(&["a\n"], resumable).await;
    f.stub.runs.lock().unwrap()[0].status = PipelineStatus::Success;
    let key = f.key("1");
    let tmp = f.h.log_store.tmp_dir().await.unwrap();
    let revision = f.h.log_store.allocate_revision();
    let epoch = f.h.log_store.epoch(f.id);
    let dir = tmp.clone();
    tokio::task::spawn_blocking(move || {
        let mut ingester = logs::ingester(
            &dir,
            key,
            revision,
            Vec::new(),
            BlockLimits {
                max_doc_text_bytes: 10,
                ..BlockLimits::default()
            },
        );
        ingester.complete = false;
        let cursor = LogCursor::new(&1).unwrap();
        logs::finish(
            logs::feed(ingester, &b"0123456789\n".repeat(3), 4),
            Some(cursor),
        );
    })
    .await
    .unwrap();
    f.h.log_store
        .publish(&f.key("1"), &tmp, revision, epoch, None, HashSet::new())
        .await
        .unwrap();

    let meta = meta_of(f.open("1").await);
    assert!(meta.truncated && !meta.complete);
    f.tick(30).await;

    assert_eq!(f.stub.log_read_count(), 0);
    until("the live task to end", || f.svc().lives.is_empty()).await;
}

#[tokio::test]
async fn a_paused_provider_is_left_alone_by_the_live_task_until_the_pause_ends() {
    let f = Fixture::live(&["a\n"], resumable).await;
    f.ready_meta("1").await;
    pause_for(&f, chrono::Duration::milliseconds(1500)).await;
    let reads = f.stub.log_read_count();
    let runs = f.stub.run_fetches.load(Ordering::SeqCst);
    let trees = f.stub.log_node_fetch_count();

    f.tick(6).await;

    assert_eq!(f.stub.log_read_count(), reads);
    assert_eq!(f.stub.run_fetches.load(Ordering::SeqCst), runs);
    assert_eq!(f.stub.log_node_fetch_count(), trees);
    std::thread::sleep(Duration::from_millis(1600));
    f.tick(6).await;
    until("polling to resume", || f.stub.log_read_count() > reads).await;
    assert!(f.stub.run_fetches.load(Ordering::SeqCst) > runs);
}

#[tokio::test]
async fn clearing_the_cache_ends_the_live_task() {
    let f = Fixture::live(&["a\n"], resumable).await;
    f.ready_meta("1").await;
    assert_eq!(f.svc().lives.len(), 1);

    f.svc().clear().await.unwrap();
    until("the live task to end", || f.svc().lives.is_empty()).await;
    let events = f.h.bus.events().len();
    f.tick(60).await;

    assert_eq!(f.stub.log_read_count(), 1);
    assert_eq!(f.h.bus.events().len(), events);
    assert!(f.svc().interests.is_empty());
}

#[tokio::test]
async fn shutting_down_ends_the_live_task() {
    let f = Fixture::live(&["a\n"], resumable).await;
    f.ready_meta("1").await;
    assert_eq!(f.svc().lives.len(), 1);

    tokio::time::timeout(Duration::from_secs(10), f.svc().shutdown())
        .await
        .expect("shutdown hung");

    assert!(f.svc().lives.is_empty() && f.svc().tracker.is_empty());
    assert!(f
        .svc()
        .open_document(&f.pipeline_id, RUN, "1", false)
        .await
        .is_err());
}

#[tokio::test]
async fn purging_the_provider_cache_ends_the_live_task_silently() {
    let f = Fixture::live(&["a\n"], resumable).await;
    f.ready_meta("1").await;
    let events = f.h.bus.events().len();

    f.h.log_store.purge_provider(f.id).await.unwrap();
    f.tick(5).await;

    until("the live task to end", || f.svc().lives.is_empty()).await;
    assert_eq!(f.stub.log_read_count(), 1);
    assert_eq!(f.h.bus.events().len(), events);
}

#[tokio::test]
async fn a_second_writer_on_the_same_store_cannot_replace_a_newer_revision() {
    let gate = Gate::new();
    let f = Fixture::with_logs(partial_nodes(), &["x\n"], gated(&gate)).await;
    let store = LogStore::open(f.h._temp_dir.path().join("run-logs"), u64::MAX)
        .await
        .unwrap();
    let other = Arc::new(LogService::new(
        store,
        Arc::clone(&f.h.provider_service),
        Arc::clone(&f.h.repository),
        f.h.bus.clone(),
    ));

    f.open("1").await;
    other
        .open_document(&f.pipeline_id, RUN, "1", false)
        .await
        .unwrap();
    until("both reads", || f.stub.log_read_count() == 2).await;
    gate.open();
    until("both attempts to end", || {
        f.svc().inflight.is_empty() && other.inflight.is_empty()
    })
    .await;

    assert_eq!(f.h.bus.count("run-log-ready"), 1);
    assert_eq!(f.h.bus.count("run-log-failed"), 0);
    assert_eq!(f.tmp_entries(), 0);
}

#[tokio::test]
async fn a_held_export_keeps_its_revision_until_the_stream_is_done() {
    let f = Fixture::live(&["line one\n"], |stub| stub).await;

    let first = f.ready_meta("1").await;
    let export = f
        .svc()
        .prepare_export(&f.pipeline_id, RUN, "1", first.revision)
        .await
        .unwrap();
    *f.stub.log_chunks.lock().unwrap() = vec![Bytes::from_static(b"line one\nline two\n")];
    f.tick(10).await;
    f.ready(2).await;
    let second = meta_of(f.open("1").await);
    let old_dir = walk(
        &f.h._temp_dir.path().join("run-logs"),
        &first.revision.to_string(),
    );

    assert_ne!(second.revision, first.revision);
    assert_eq!(second.line_count, 2);
    f.h.log_store.sweep().await.unwrap();
    assert!(old_dir.exists());
    let bytes: Vec<u8> = export
        .into_stream()
        .unwrap()
        .map(|item| item.unwrap())
        .collect::<Vec<_>>()
        .await
        .concat();
    assert_eq!(bytes, b"line one\n");
    for _ in 0..100 {
        f.h.log_store.sweep().await.unwrap();
        if !old_dir.exists() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(!old_dir.exists());
}

#[tokio::test]
async fn terminal_trees_are_saved_and_refetched_when_the_run_concludes_again() {
    let f = Fixture::new(|stub| stub).await;

    f.svc().run_log_tree(&f.pipeline_id, RUN).await.unwrap();
    assert!(f
        .h
        .log_store
        .read_tree(f.id, &f.pipeline_id, RUN)
        .await
        .is_some());
    f.svc().run_log_tree(&f.pipeline_id, RUN).await.unwrap();
    assert_eq!(f.stub.log_node_fetch_count(), 1);

    f.stub.runs.lock().unwrap()[0].concluded_at = Some(Utc::now());
    f.svc().run_log_tree(&f.pipeline_id, RUN).await.unwrap();
    assert_eq!(f.stub.log_node_fetch_count(), 2);
}

#[tokio::test]
async fn a_complete_tree_memo_expires_after_a_minute() {
    let f = Fixture::new(|stub| stub).await;
    f.warm().await;
    tokio::time::pause();

    f.ready_meta("1").await;
    let fetches = f.stub.run_fetches.load(Ordering::SeqCst);
    f.open("1").await;
    assert_eq!(f.stub.run_fetches.load(Ordering::SeqCst), fetches);

    tokio::time::advance(Duration::from_secs(61)).await;
    f.open("1").await;
    assert_eq!(f.stub.run_fetches.load(Ordering::SeqCst), fetches + 1);
}

#[tokio::test]
async fn sessions_search_independently_and_a_newer_search_replaces_that_sessions_older_one() {
    let f = Fixture::new(|stub| stub).await;
    let meta = f.ready_meta("1").await;
    let svc = Arc::clone(f.svc());
    let permits = Arc::clone(&svc.search_permits)
        .acquire_many_owned(SEARCH_PERMITS as u32)
        .await
        .unwrap();
    let spawn_search = |session: &'static str| {
        let svc = Arc::clone(&svc);
        let pipeline_id = f.pipeline_id.clone();
        let revision = meta.revision;
        tokio::spawn(async move {
            let query = LogSearchQuery {
                text: "LINE".into(),
                regex: false,
                case_sensitive: false,
            };
            svc.search(&pipeline_id, RUN, "1", revision, session, &query)
                .await
        })
    };

    let first = spawn_search("a");
    until("session a", || svc.searches.len() == 1).await;
    let other = spawn_search("b");
    until("session b", || svc.searches.len() == 2).await;
    let newer = spawn_search("a");

    assert!(first.await.unwrap().is_err());
    drop(permits);
    assert_eq!(other.await.unwrap().unwrap().total, 2);
    assert_eq!(newer.await.unwrap().unwrap().total, 2);
}

#[tokio::test]
async fn search_rejects_bad_queries_before_touching_the_cache() {
    let f = Fixture::new(|stub| stub).await;
    let search = |text: &str, regex: bool| {
        let query = LogSearchQuery {
            text: text.to_string(),
            regex,
            case_sensitive: false,
        };
        let svc = Arc::clone(f.svc());
        let pipeline_id = f.pipeline_id.clone();
        async move { svc.search(&pipeline_id, RUN, "1", 1, "s", &query).await }
    };

    assert_eq!(search("", false).await.unwrap().total, 0);
    assert!(matches!(
        search("(", true).await,
        Err(DomainError::InvalidConfig(m)) if m.starts_with("Invalid search pattern")
    ));
    assert!(matches!(
        search(&"a".repeat(SEARCH_MAX_QUERY_CHARS + 1), false).await,
        Err(DomainError::InvalidConfig(m)) if m == "Search text is too long"
    ));
}

#[tokio::test]
async fn reading_a_revision_that_is_gone_is_not_found() {
    let f = Fixture::new(|stub| stub).await;
    let meta = f.ready_meta("1").await;

    let err = f
        .svc()
        .read_lines(&f.pipeline_id, RUN, "1", meta.revision + 1, 0, 10, 4096)
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::NotFound(m) if m == REVISION_GONE));
}

#[tokio::test]
async fn an_export_over_a_corrupt_block_yields_an_error() {
    let f = Fixture::new(|stub| stub).await;
    let meta = f.ready_meta("1").await;
    let blocks = walk(&f.h._temp_dir.path().join("run-logs"), "blocks.bin");
    let size = std::fs::metadata(&blocks).unwrap().len() as usize;
    std::fs::write(&blocks, vec![0xFF; size]).unwrap();

    let stream = f
        .svc()
        .prepare_export(&f.pipeline_id, RUN, "1", meta.revision)
        .await
        .unwrap()
        .into_stream()
        .unwrap();
    let items: Vec<_> = stream.collect().await;

    assert!(items.iter().any(|item| item.is_err()));
}

#[tokio::test]
async fn an_export_streams_the_rendered_text() {
    let f = Fixture::new(|stub| stub).await;
    let meta = f.ready_meta("1").await;

    let stream = f
        .svc()
        .prepare_export(&f.pipeline_id, RUN, "1", meta.revision)
        .await
        .unwrap()
        .into_stream()
        .unwrap();
    let bytes: Vec<u8> = stream
        .map(|item| item.unwrap())
        .collect::<Vec<_>>()
        .await
        .concat();

    assert_eq!(bytes, b"line one\nline two\n");
}

#[tokio::test]
async fn provider_failures_are_worded_remembered_and_retryable() {
    let f = Fixture::new(|stub| stub).await;
    *f.stub.log_failure.lock().unwrap() = Some(DomainError::RateLimited {
        message: "slow down".into(),
        retry_after: Some(Duration::from_secs(90)),
    });

    assert!(matches!(f.open("1").await, RunLogDocument::Loading { .. }));
    f.failed(1).await;
    until("the attempt to leave", || f.svc().inflight.is_empty()).await;
    let message = "Rate limit reached. Try again in 2 min.";

    assert!(matches!(
        f.open("1").await,
        RunLogDocument::Unavailable { message: m } if m == message
    ));
    assert_eq!(f.stub.log_read_count(), 1);
    *f.stub.log_failure.lock().unwrap() = None;
    assert!(matches!(
        f.open_with("1", true).await,
        RunLogDocument::Loading { .. }
    ));
    f.ready(1).await;
    assert_eq!(f.stub.log_read_count(), 2);
}

#[tokio::test]
async fn opening_nodes_that_cannot_have_a_log_is_not_found() {
    let f = Fixture::new(|stub| stub).await;
    let open = |node_id: &'static str| {
        let svc = Arc::clone(f.svc());
        let pipeline_id = f.pipeline_id.clone();
        async move { svc.open_document(&pipeline_id, RUN, node_id, false).await }
    };

    assert!(matches!(
        open("missing").await,
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(open("1:1").await, Err(DomainError::NotFound(_))));
}

#[tokio::test]
async fn providers_without_the_logs_capability_are_not_supported() {
    let h = Harness::new().await;
    let (id, _) = h.add_stub(StubProvider::new).await;
    h.repository
        .update_pipelines_cache(id, &[pipeline(id, "web", PipelineStatus::Success)])
        .await
        .unwrap();

    let err = h
        .log_service
        .run_log_tree(&format!("github__{id}__web"), RUN)
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::NotSupported(_)));
}

#[tokio::test]
async fn an_unknown_pipeline_is_not_found() {
    let h = Harness::new().await;

    let err = h
        .log_service
        .run_log_tree("github__1__missing", RUN)
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::PipelineNotFound(_)));
}

async fn pause(f: &Fixture) {
    pause_for(f, chrono::Duration::minutes(10)).await;
}

async fn pause_for(f: &Fixture, duration: chrono::Duration) {
    let lease = f.h.provider_service.acquire(f.id).await.unwrap();
    f.h.scheduler.set_quota_pause(
        f.id,
        lease.generation,
        Some(Utc::now() + duration),
        Instant::now(),
        Utc::now(),
    );
}

#[tokio::test]
async fn a_cold_open_fetches_the_run_once() {
    let f = Fixture::new(|stub| stub).await;

    f.ready_meta("1").await;

    assert_eq!(f.stub.run_fetches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_paused_provider_is_not_called_for_the_tree() {
    let f = Fixture::new(|stub| stub).await;
    pause(&f).await;

    let err = f
        .svc()
        .open_document(&f.pipeline_id, RUN, "1", false)
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::RateLimited { .. }));
    assert_eq!(f.stub.run_fetches.load(Ordering::SeqCst), 0);
    assert_eq!(f.stub.log_node_fetch_count(), 0);
    assert_eq!(f.stub.log_read_count(), 0);
}

#[tokio::test]
async fn a_paused_provider_fails_an_ingest_without_reading() {
    let f = Fixture::new(|stub| stub).await;
    f.ready_meta("1").await;
    pause(&f).await;

    assert!(matches!(f.open("2").await, RunLogDocument::Loading { .. }));
    f.failed(1).await;

    assert!(matches!(
        f.open("2").await,
        RunLogDocument::Unavailable { .. }
    ));
    assert_eq!(f.stub.log_read_count(), 1);
}

#[tokio::test]
async fn a_rate_limited_read_pauses_a_provider_whose_quota_is_nearly_used_up() {
    let f = Fixture::new(|stub| stub).await;
    *f.stub.rate_limit.lock().unwrap() = Some(pipedash_plugin_api::RateLimit {
        limit: 5000,
        remaining: 10,
        reset_at: Utc::now() + chrono::Duration::minutes(20),
    });
    *f.stub.log_failure.lock().unwrap() = Some(DomainError::RateLimited {
        message: "slow down".into(),
        retry_after: Some(Duration::from_secs(60)),
    });

    f.open("1").await;
    f.failed(1).await;

    assert!(f.h.scheduler.paused_until(f.id, Utc::now()).is_some());
    assert!(matches!(
        f.svc().open_document(&f.pipeline_id, RUN, "1", true).await,
        Ok(RunLogDocument::Loading { .. })
    ));
    f.failed(2).await;
    assert_eq!(f.stub.log_read_count(), 1);
}

#[test]
fn trees_drop_duplicates_and_orphans_and_pick_the_failed_job() {
    let run = stub_run("p", 1, 1);
    let nodes = vec![
        log_node(
            "stage",
            None,
            LogNodeKind::Stage,
            PipelineStatus::Failed,
            None,
        ),
        log_node(
            "a",
            Some("stage"),
            LogNodeKind::Job,
            PipelineStatus::Success,
            Some(LogAvailability::Ready),
        ),
        log_node(
            "b",
            Some("stage"),
            LogNodeKind::Job,
            PipelineStatus::Failed,
            Some(LogAvailability::Ready),
        ),
        job("a", PipelineStatus::Failed, LogAvailability::Ready),
        log_node(
            "orphan",
            Some("later"),
            LogNodeKind::Job,
            PipelineStatus::Success,
            None,
        ),
        step("b:1", "b"),
    ];

    let tree = tree_from_nodes(&run, nodes);

    assert_eq!(tree.nodes.len(), 5);
    assert_eq!(tree.default_node_id.as_deref(), Some("b"));
    assert!(tree.complete);
    let by_id = |id: &str| tree.nodes.iter().find(|node| node.id == id).unwrap();
    assert_eq!(by_id("a").depth, 1);
    assert!(by_id("stage").navigable);
    assert!(by_id("b").navigable);
    assert!(!by_id("b:1").navigable);
    assert!(!by_id("orphan").navigable);
    assert_eq!(by_id("orphan").parent_id, None);
}

#[test]
fn trees_with_live_or_partial_nodes_are_incomplete() {
    let run = stub_run("p", 1, 1);
    let tree = tree_from_nodes(
        &run,
        vec![
            job("1", PipelineStatus::Running, LogAvailability::Pending),
            job("2", PipelineStatus::Success, LogAvailability::Partial),
        ],
    );

    assert!(!tree.complete);
    assert_eq!(tree.default_node_id.as_deref(), Some("2"));
}

#[test]
fn run_log_events_carry_camel_case_payloads() {
    let ready = CoreEvent::RunLogReady {
        pipeline_id: "p".into(),
        run_number: 3,
        node_id: "n".into(),
        revision: 9,
    };

    assert_eq!(ready.event_name(), "run-log-ready");
    assert_eq!(
        ready.to_json_payload(),
        serde_json::json!({ "pipelineId": "p", "runNumber": 3, "nodeId": "n", "revision": 9 })
    );
}

#[test]
fn live_run_events_carry_camel_case_payloads() {
    let changed = CoreEvent::RunLogTreeChanged {
        pipeline_id: "p".into(),
        run_number: 3,
    };
    assert_eq!(changed.event_name(), "run-log-tree-changed");
    assert_eq!(
        changed.to_json_payload(),
        serde_json::json!({ "pipelineId": "p", "runNumber": 3 })
    );

    let run = stub_run("p", 3, 12);
    let updated = CoreEvent::RunUpdated {
        pipeline_id: "p".into(),
        run_number: 3,
        run: run.clone(),
    };
    assert_eq!(updated.event_name(), "run-updated");
    assert_eq!(
        updated.to_json_payload(),
        serde_json::json!({
            "pipelineId": "p",
            "runNumber": 3,
            "run": serde_json::to_value(&run).unwrap(),
        })
    );
}

fn walk(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|file| file == name) {
                return path;
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    panic!("no {name} under the cache");
}
