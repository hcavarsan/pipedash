use std::fs::{
    File,
    FileTimes,
};

use tempfile::TempDir;

use super::super::checkpoint::{
    CHECKPOINT_FILE,
    TAIL_FILE,
};
use super::super::ingest::{
    INDEX_FILE,
    META_FILE,
};
use super::super::testing::{
    doc_key,
    feed,
    finish,
    ingester,
};
use super::super::{
    BlockLimits,
    CACHE_MAX_IDLE,
};
use super::registry::lock;
use super::*;
use crate::domain::{
    LineKind,
    LogCursor,
};

fn root(dir: &TempDir) -> PathBuf {
    dir.path().join("run-logs")
}

fn key_n(run: i64) -> DocKey {
    DocKey {
        run_number: run,
        ..doc_key()
    }
}

fn rev_dir(store: &LogStore, key: &DocKey, revision: i64) -> PathBuf {
    store
        .run_dir(key.provider_id, &key.run_key())
        .join(DOCS_DIR)
        .join(key.doc_key())
        .join(revision.to_string())
}

async fn publish_doc(store: &Arc<LogStore>, key: &DocKey, text: &str) -> i64 {
    publish_busy(store, key, text, HashSet::new()).await
}

async fn prepare(
    store: &Arc<LogStore>, key: &DocKey, text: &str, complete: bool,
) -> (PathBuf, i64, CacheEpoch) {
    let revision = store.allocate_revision();
    let epoch = store.epoch(key.provider_id);
    let tmp = store.tmp_dir().await.expect("tmp dir");
    let base = store.base(key).await.expect("base");
    let (dir, key2, data) = (tmp.clone(), key.clone(), text.as_bytes().to_vec());
    tokio::task::spawn_blocking(move || {
        let mut ingester = ingester(&dir, key2, revision, Vec::new(), BlockLimits::default());
        ingester.complete = complete;
        ingester.base = base;
        let cursor = LogCursor::new(&revision).expect("cursor");
        finish(feed(ingester, &data, 1 << 16), Some(cursor));
    })
    .await
    .expect("ingest");

    (tmp, revision, epoch)
}

async fn publish_busy(
    store: &Arc<LogStore>, key: &DocKey, text: &str, busy: HashSet<RunId>,
) -> i64 {
    let (tmp, revision, epoch) = prepare(store, key, text, true).await;
    let outcome = store
        .publish(key, &tmp, revision, epoch, store.current(key), busy)
        .await
        .expect("publish");
    assert_eq!(outcome, PublishOutcome::Published);
    revision
}

async fn publish_incomplete(store: &Arc<LogStore>, key: &DocKey, text: &str) -> i64 {
    let (tmp, revision, epoch) = prepare(store, key, text, false).await;
    let outcome = store
        .publish(
            key,
            &tmp,
            revision,
            epoch,
            store.current(key),
            HashSet::new(),
        )
        .await
        .expect("publish");
    assert_eq!(outcome, PublishOutcome::Published);
    revision
}

fn files_of(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            let path = entry.expect("entry").path();
            let name = path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned();
            (name, fs::read(&path).expect("read"))
        })
        .collect()
}

async fn lines_of(store: &Arc<LogStore>, key: &DocKey, revision: i64) -> Vec<String> {
    let (doc, _lease) = store
        .lease(key, revision)
        .await
        .expect("lease")
        .expect("some");
    let page = doc
        .read_lines(store.block_cache(), 0, 100, 4096)
        .expect("page");
    page.lines.into_iter().map(|line| line.text).collect()
}

async fn first_line(store: &Arc<LogStore>, key: &DocKey, revision: i64) -> Option<String> {
    let (doc, _lease) = store.lease(key, revision).await.expect("lease")?;
    let page = doc
        .read_lines(store.block_cache(), 0, 1, 4096)
        .expect("page");
    page.lines.first().map(|l| l.text.clone())
}

#[tokio::test]
async fn run_logs_store_publish_survives_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let revision = publish_doc(&store, &key, "hello\nworld\n").await;
    assert_eq!(store.current(&key), Some(revision));
    drop(store);

    let store = LogStore::open(root(&dir), u64::MAX).await.expect("reopen");
    assert_eq!(store.current(&key), Some(revision));
    assert_eq!(
        first_line(&store, &key, revision).await.as_deref(),
        Some("hello")
    );
    assert!(store.allocate_revision() > revision);
    let (doc, _lease) = store
        .lease(&key, revision)
        .await
        .expect("lease")
        .expect("some");
    assert_eq!(doc.meta().to_meta().line_count, 2);
    assert_eq!(doc.meta().key, key);
    let _ = LineKind::Plain;
}

#[tokio::test]
async fn run_logs_store_drops_documents_indexed_before_error_inference() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let revision = publish_doc(&store, &key, "ERROR: Job failed: exit code 1\n").await;
    let meta_path = rev_dir(&store, &key, revision).join(META_FILE);
    let mut old: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_path).expect("meta")).expect("json");
    old["format"] = serde_json::json!(1);
    old["error_lines"] = serde_json::json!([]);
    old["error_total"] = serde_json::json!(0);
    fs::write(&meta_path, serde_json::to_vec(&old).expect("encode")).expect("write");
    drop(store);

    let store = LogStore::open(root(&dir), u64::MAX).await.expect("reopen");

    assert_eq!(store.current(&key), None);
    assert!(!rev_dir(&store, &key, revision).exists());
}

#[tokio::test]
async fn run_logs_store_drops_documents_stored_before_checkpoints() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let revision = publish_incomplete(&store, &key, "one\ntwo").await;
    let meta_path = rev_dir(&store, &key, revision).join(META_FILE);
    let mut old: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_path).expect("meta")).expect("json");
    old["format"] = serde_json::json!(2);
    fs::write(&meta_path, serde_json::to_vec(&old).expect("encode")).expect("write");
    drop(store);

    let store = LogStore::open(root(&dir), u64::MAX).await.expect("reopen");

    assert_eq!(store.current(&key), None);
    assert!(!rev_dir(&store, &key, revision).exists());
}

#[tokio::test]
async fn run_logs_store_publish_rejects_a_stale_base() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let first = publish_doc(&store, &key, "one\n").await;

    let (tmp, revision, epoch) = prepare(&store, &key, "late\n", true).await;
    let empty_base = store
        .publish(&key, &tmp, revision, epoch, None, HashSet::new())
        .await
        .expect("publish");

    assert_eq!(empty_base, PublishOutcome::StaleBase);
    assert_eq!(store.current(&key), Some(first));
    assert!(tmp.is_dir());
    assert!(!rev_dir(&store, &key, revision).exists());
    let second = publish_doc(&store, &key, "two\n").await;
    let moved_on = store
        .publish(&key, &tmp, revision, epoch, Some(first), HashSet::new())
        .await
        .expect("publish");
    assert_eq!(moved_on, PublishOutcome::StaleBase);
    assert_eq!(store.current(&key), Some(second));
    let current = store
        .publish(&key, &tmp, revision, epoch, Some(second), HashSet::new())
        .await
        .expect("publish");
    assert_eq!(current, PublishOutcome::Published);
    assert_eq!(store.current(&key), Some(revision));
    assert_eq!(lines_of(&store, &key, revision).await, ["late"]);
}

#[tokio::test]
async fn run_logs_store_resuming_never_touches_the_leased_base() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let first = publish_incomplete(&store, &key, "one\ntwo\npart").await;
    let base = store.base(&key).await.expect("base").expect("some");
    assert!(base.cursor(&[]).is_some());
    let base_dir = base.document.dir().to_path_buf();
    let before = files_of(&base_dir);

    let second = publish_incomplete(&store, &key, "ial\nthree\n").await;

    assert_ne!(first, second);
    assert_eq!(store.current(&key), Some(second));
    assert!(base_dir.is_dir());
    assert_eq!(files_of(&base_dir), before);
    let held = base
        .document
        .read_lines(store.block_cache(), 0, 100, 4096)
        .expect("page");
    let held: Vec<_> = held.lines.into_iter().map(|line| line.text).collect();
    assert_eq!(held, ["one", "two", "part"]);
    assert_eq!(
        lines_of(&store, &key, second).await,
        ["one", "two", "partial", "three"]
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let blocks = |revision| fs::metadata(rev_dir(&store, &key, revision).join("blocks.bin"));
        let (old, new) = (blocks(first).unwrap(), blocks(second).unwrap());
        assert_eq!((old.nlink(), new.nlink()), (1, 1));
        assert_ne!(old.ino(), new.ino());
    }

    drop(base);
    store.sweep().await.expect("sweep");
    assert!(!base_dir.exists());
}

#[tokio::test]
async fn run_logs_store_budget_counts_the_checkpoint_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let text = format!("##[error]boom\n{}", "x".repeat(300_000));
    let revision = publish_incomplete(&store, &key, &text).await;
    let files = files_of(&rev_dir(&store, &key, revision));

    for name in [CHECKPOINT_FILE, TAIL_FILE, "errors.bin"] {
        assert!(files.contains_key(name), "{name} is missing");
    }
    let total: usize = files.values().map(Vec::len).sum();
    assert!(files[TAIL_FILE].len() >= 300_000);
    assert_eq!(store.stats().await.bytes, total as u64);
}

#[tokio::test]
async fn run_logs_store_second_open_shares_instance_and_temp_dirs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let tmp = first.tmp_dir().await.expect("tmp");
    let second = LogStore::open(root(&dir), 1).await.expect("open again");
    assert!(Arc::ptr_eq(&first, &second));
    assert!(tmp.is_dir());
    drop((first, second));
    let third = LogStore::open(root(&dir), u64::MAX).await.expect("reopen");
    assert!(!tmp.exists());
    drop(third);
}

#[tokio::test]
async fn run_logs_store_keeps_leased_revision_readable_until_dropped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let a = publish_doc(&store, &key, "alpha\n").await;
    let (doc, lease) = store.lease(&key, a).await.expect("lease").expect("some");
    let b = publish_doc(&store, &key, "beta\n").await;
    assert_eq!(store.current(&key), Some(b));
    let page = doc
        .read_lines(store.block_cache(), 0, 1, 4096)
        .expect("page");
    assert_eq!(page.lines[0].text, "alpha");
    assert!(store.lease(&key, a).await.expect("lease").is_none());
    store.sweep().await.expect("sweep");
    assert!(rev_dir(&store, &key, a).is_dir());

    drop(lease);
    store.sweep().await.expect("sweep");
    assert!(!rev_dir(&store, &key, a).exists());
    assert_eq!(first_line(&store, &key, b).await.as_deref(), Some("beta"));
}

#[tokio::test]
async fn run_logs_store_purge_with_lease_keeps_new_revision() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let a = publish_doc(&store, &key, "alpha\n").await;
    let (_doc, lease) = store.lease(&key, a).await.expect("lease").expect("some");
    store.purge_provider(key.provider_id).await.expect("purge");
    assert_eq!(
        store.epoch(key.provider_id),
        CacheEpoch {
            global: 0,
            provider: 1
        }
    );
    assert_eq!(store.current(&key), None);
    assert!(rev_dir(&store, &key, a).is_dir());

    let b = publish_doc(&store, &key, "beta\n").await;
    drop(lease);
    store.sweep().await.expect("sweep");
    assert!(!rev_dir(&store, &key, a).exists());
    assert_eq!(first_line(&store, &key, b).await.as_deref(), Some("beta"));
    assert!(rev_dir(&store, &key, b).is_dir());
}

#[tokio::test]
async fn run_logs_store_failed_lease_load_leaves_no_readers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let revision = publish_doc(&store, &key, "alpha\nbeta\n").await;
    let index = rev_dir(&store, &key, revision).join(INDEX_FILE);
    let mut bytes = fs::read(&index).expect("index");
    bytes.push(0);
    fs::write(&index, bytes).expect("write");

    assert!(store.lease(&key, revision).await.expect("lease").is_none());
    assert_eq!(store.current(&key), None);
    {
        let reg = lock(&store.registry);
        assert_eq!(reg.retired.len(), 1);
        assert_eq!(reg.retired[0].readers.load(Ordering::SeqCst), 0);
    }
    store.sweep().await.expect("sweep");
    assert!(!rev_dir(&store, &key, revision).exists());
    assert!(lock(&store.registry).retired.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn run_logs_store_delete_failure_is_requeued() {
    use std::os::unix::fs::PermissionsExt;

    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let revision = publish_doc(&store, &key, "alpha\n").await;
    let target = rev_dir(&store, &key, revision);
    let parent = target.parent().expect("parent").to_path_buf();
    let _restore = Restore(parent.clone());
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).expect("chmod");
    if fs::create_dir(parent.join("probe")).is_ok() {
        return;
    }

    assert!(store.clear().await.is_err());
    assert!(target.exists());
    assert_eq!(lock(&store.deletes).len(), 1);
    assert!(store.stats().await.bytes > 0 || target.exists());

    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).expect("chmod");
    store.sweep().await.expect("sweep");
    assert!(!target.exists());
    assert_eq!(store.stats().await.bytes, 0);
}

async fn doc_bytes(text: &str) -> u64 {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    publish_doc(&store, &doc_key(), text).await;
    let bytes = store.stats().await.bytes;
    drop(store);
    bytes
}

fn set_access(store: &LogStore, key: &DocKey, age: Duration) {
    let mut reg = lock(&store.registry);
    let run = reg.runs.get_mut(&key.run_id()).expect("run");
    run.last_access = SystemTime::now() - age;
}

#[tokio::test]
async fn run_logs_store_evicts_least_recently_used_and_skips_leased() {
    let text = "a line of log output\nanother line\n";
    let size = doc_bytes(text).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), size * 3 + size / 2)
        .await
        .expect("open");
    let hour = Duration::from_secs(3600);
    let keys: Vec<DocKey> = (1..=4).map(key_n).collect();
    let mut revisions = Vec::new();
    for (key, age) in keys.iter().zip([4, 3, 2]) {
        revisions.push(publish_doc(&store, key, text).await);
        set_access(&store, key, hour * age);
    }
    let (_doc, lease) = store
        .lease(&keys[0], revisions[0])
        .await
        .expect("lease")
        .expect("some");
    set_access(&store, &keys[0], hour * 4);

    revisions.push(publish_doc(&store, &keys[3], text).await);
    assert!(store.current(&keys[0]).is_some());
    assert!(store.current(&keys[1]).is_none());
    assert!(store.current(&keys[2]).is_some());
    assert!(store.current(&keys[3]).is_some());
    drop(lease);
}

#[tokio::test]
async fn run_logs_store_eviction_skips_busy_runs() {
    let text = "a line of log output\nanother line\n";
    let size = doc_bytes(text).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), size * 3 + size / 2)
        .await
        .expect("open");
    let hour = Duration::from_secs(3600);
    let keys: Vec<DocKey> = (1..=4).map(key_n).collect();
    for (key, age) in keys.iter().zip([4, 3, 2]) {
        publish_doc(&store, key, text).await;
        set_access(&store, key, hour * age);
    }
    let busy = HashSet::from([keys[0].run_id()]);
    publish_busy(&store, &keys[3], text, busy).await;
    assert!(store.current(&keys[0]).is_some());
    assert!(store.current(&keys[1]).is_none());
    assert!(store.current(&keys[2]).is_some());
    store.sweep().await.expect("sweep");
    assert!(!lock(&store.registry).runs.contains_key(&keys[1].run_id()));
    assert!(store.stats().await.bytes <= size * 3 + size / 2);
}

#[tokio::test]
async fn run_logs_store_epochs_reject_stale_publish_and_tree_writes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let tree = StoredTree {
        log_tree_version: 0,
        run_status: PipelineStatus::Success,
        run_concluded_at: None,
        tree: RunLogTree {
            nodes: Vec::new(),
            default_node_id: None,
            complete: true,
            groups: Vec::new(),
            step_ids_by_node: Default::default(),
        },
    };
    let stale = store.epoch(key.provider_id);
    store
        .write_tree(
            key.provider_id,
            &key.pipeline_id,
            key.run_number,
            &tree,
            stale,
        )
        .await
        .expect("write tree");
    let read = store
        .read_tree(key.provider_id, &key.pipeline_id, key.run_number)
        .await;
    assert!(read.is_some_and(|t| t.tree.complete));

    for purge_provider in [true, false] {
        let epoch = store.epoch(key.provider_id);
        if purge_provider {
            store.purge_provider(key.provider_id).await.expect("purge");
        } else {
            store.clear().await.expect("clear");
        }
        assert_ne!(store.epoch(key.provider_id), epoch);
        let tmp = store.tmp_dir().await.expect("tmp");
        let outcome = store
            .publish(&key, &tmp, 5, epoch, None, HashSet::new())
            .await
            .expect("publish");
        assert_eq!(outcome, PublishOutcome::StaleEpoch);
        store
            .write_tree(
                key.provider_id,
                &key.pipeline_id,
                key.run_number,
                &tree,
                epoch,
            )
            .await
            .expect("stale tree write");
        assert!(store
            .read_tree(key.provider_id, &key.pipeline_id, key.run_number)
            .await
            .is_none());
        store.discard(tmp.clone()).await;
        assert!(!tmp.exists());
    }
    assert_eq!(
        store.epoch(key.provider_id),
        CacheEpoch {
            global: 1,
            provider: 1
        }
    );
}

#[tokio::test]
async fn run_logs_store_revisions_increase_when_the_clock_goes_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    assert_eq!(store.allocate_at(5_000), 5_000);
    assert_eq!(store.allocate_at(1_000), 5_001);
    assert_eq!(store.allocate_at(5_000), 5_002);
    assert!(store.allocate_revision() > 5_002);
}

#[tokio::test]
async fn run_logs_store_touch_advances_with_zero_interval() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = doc_key();
    let store = LogStore::open_with(root(&dir), u64::MAX, Duration::ZERO)
        .await
        .expect("open");
    let revision = publish_doc(&store, &key, "alpha\n").await;
    let touch = store
        .run_dir(key.provider_id, &key.run_key())
        .join(TOUCH_FILE);
    let old = SystemTime::now() - Duration::from_secs(10 * 24 * 3600);
    File::options()
        .write(true)
        .open(&touch)
        .expect("touch file")
        .set_times(FileTimes::new().set_modified(old))
        .expect("set times");
    let _ = first_line(&store, &key, revision).await;
    let modified = fs::metadata(&touch)
        .and_then(|m| m.modified())
        .expect("mtime");
    assert!(modified > old + Duration::from_secs(24 * 3600));
}

#[tokio::test]
async fn run_logs_store_scan_keeps_newest_revision_and_drops_idle_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (live, idle) = (key_n(1), key_n(2));
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let first = publish_doc(&store, &live, "one\n").await;
    let second = publish_doc(&store, &live, "two\n").await;
    publish_doc(&store, &idle, "idle\n").await;
    let touch = store
        .run_dir(idle.provider_id, &idle.run_key())
        .join(TOUCH_FILE);
    let old = SystemTime::now() - CACHE_MAX_IDLE - Duration::from_secs(3600);
    File::options()
        .write(true)
        .open(&touch)
        .expect("touch")
        .set_times(FileTimes::new().set_modified(old))
        .expect("set times");
    let stray = root(&dir).join("old-layout");
    fs::create_dir_all(&stray).expect("stray");
    let first_dir = rev_dir(&store, &live, first);
    drop(store);

    let store = LogStore::open(root(&dir), u64::MAX).await.expect("reopen");
    assert!(!stray.exists());
    assert!(!first_dir.exists());
    assert_eq!(store.current(&live), Some(second));
    assert_eq!(store.current(&idle), None);
    assert_eq!(
        first_line(&store, &live, second).await.as_deref(),
        Some("two")
    );
    assert_eq!(store.stats().await.runs, 1);
}

const DOC_TEXT: &str = "a line of log output\nanother line\n";

fn node_key(node: usize) -> DocKey {
    DocKey {
        node_id: format!("job-{node}"),
        ..doc_key()
    }
}

#[tokio::test]
async fn run_logs_store_trims_docs_inside_one_huge_run() {
    let size = doc_bytes(DOC_TEXT).await;
    let budget = size * 2 + size / 2;
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), budget).await.expect("open");
    let keys: Vec<DocKey> = (0..5).map(node_key).collect();
    for key in &keys {
        publish_doc(&store, key, DOC_TEXT).await;
    }
    assert!(store.stats().await.bytes <= budget);
    let last = store.current(&keys[4]).expect("newest doc survives");
    assert_eq!(
        first_line(&store, &keys[4], last).await.as_deref(),
        Some("a line of log output")
    );
    assert!(store.current(&keys[0]).is_none());
}

#[tokio::test]
async fn run_logs_store_trim_in_one_run_skips_leased_docs() {
    let size = doc_bytes(DOC_TEXT).await;
    let budget = size * 2 + size / 2;
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), budget).await.expect("open");
    let keys: Vec<DocKey> = (0..5).map(node_key).collect();
    let first = publish_doc(&store, &keys[0], DOC_TEXT).await;
    let (doc, lease) = store
        .lease(&keys[0], first)
        .await
        .expect("lease")
        .expect("some");
    for key in &keys[1..] {
        publish_doc(&store, key, DOC_TEXT).await;
    }
    assert_eq!(store.current(&keys[0]), Some(first));
    assert!(store.current(&keys[4]).is_some());
    assert!(store.current(&keys[1]).is_none());
    assert!(store.stats().await.bytes <= budget);
    let page = doc
        .read_lines(store.block_cache(), 0, 1, 4096)
        .expect("page");
    assert_eq!(page.lines[0].text, "a line of log output");
    drop(lease);
}

#[tokio::test]
async fn run_logs_store_sweep_trims_once_docs_are_unleased_and_not_busy() {
    let size = doc_bytes(DOC_TEXT).await;
    let budget = size * 2 + size / 2;
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), budget).await.expect("open");
    let keys: Vec<DocKey> = (0..3).map(node_key).collect();
    let mut leases = Vec::new();
    for key in &keys[..2] {
        let revision = publish_doc(&store, key, DOC_TEXT).await;
        let held = store
            .lease(key, revision)
            .await
            .expect("lease")
            .expect("some");
        leases.push(held);
    }
    let revision = publish_doc(&store, &keys[2], DOC_TEXT).await;
    let newest = store
        .lease(&keys[2], revision)
        .await
        .expect("lease")
        .expect("some");
    assert!(store.stats().await.bytes > budget);

    drop(leases);
    let busy = HashSet::from([keys[0].run_id()]);
    store.sweep_with(busy, None).await.expect("sweep");
    assert!(store.stats().await.bytes > budget);

    store.sweep_with(HashSet::new(), None).await.expect("sweep");
    assert!(store.stats().await.bytes <= budget);
    assert!(store.current(&keys[0]).is_none());
    assert!(store.current(&keys[1]).is_some());
    assert!(store.current(&keys[2]).is_some());
    drop(newest);
}

#[tokio::test]
async fn run_logs_store_sweep_expires_idle_runs_but_not_leased_or_busy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open_with(root(&dir), u64::MAX, Duration::ZERO)
        .await
        .expect("open");
    let keys: Vec<DocKey> = (1..=3).map(key_n).collect();
    let mut revisions = Vec::new();
    for key in &keys {
        revisions.push(publish_doc(&store, key, DOC_TEXT).await);
    }
    let held = store
        .lease(&keys[1], revisions[1])
        .await
        .expect("lease")
        .expect("some");
    let idle = CACHE_MAX_IDLE + Duration::from_secs(3600);
    for key in &keys {
        set_access(&store, key, idle);
    }

    let busy = HashSet::from([keys[2].run_id()]);
    store.sweep_with(busy, None).await.expect("sweep");
    assert!(store.current(&keys[0]).is_none());
    assert!(store.current(&keys[1]).is_some());
    assert!(store.current(&keys[2]).is_some());
    assert!(!rev_dir(&store, &keys[0], revisions[0]).exists());
    assert!(rev_dir(&store, &keys[1], revisions[1]).is_dir());
    drop(held);
}

#[tokio::test]
async fn run_logs_store_idle_expiry_is_rate_limited() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), u64::MAX).await.expect("open");
    let key = doc_key();
    publish_doc(&store, &key, DOC_TEXT).await;
    store.sweep_with(HashSet::new(), None).await.expect("sweep");
    set_access(&store, &key, CACHE_MAX_IDLE + Duration::from_secs(3600));
    store.sweep_with(HashSet::new(), None).await.expect("sweep");
    assert!(store.current(&key).is_some());
}

#[tokio::test]
async fn run_logs_store_sweep_keeps_the_document_being_read_even_over_budget() {
    let size = doc_bytes(DOC_TEXT).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(root(&dir), size / 2).await.expect("open");
    let key = doc_key();
    publish_doc(&store, &key, DOC_TEXT).await;
    assert!(store.stats().await.bytes > size / 2);

    store
        .sweep_with(HashSet::new(), Some(&key))
        .await
        .expect("sweep");
    assert!(store.current(&key).is_some());

    store.sweep_with(HashSet::new(), None).await.expect("sweep");
    assert!(store.current(&key).is_none());
}
