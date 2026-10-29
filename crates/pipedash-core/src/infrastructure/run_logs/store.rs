use std::collections::{
    BTreeMap,
    HashMap,
    HashSet,
};
use std::fs::{
    self,
    OpenOptions,
};
use std::io;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::atomic::{
    AtomicBool,
    AtomicU32,
    Ordering,
};
use std::sync::{
    Arc,
    LazyLock,
    Mutex,
    MutexGuard,
    PoisonError,
    Weak,
};
use std::time::{
    Duration,
    SystemTime,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};

use super::checkpoint::{
    Base,
    Checkpoint,
};
use super::document::{
    BlockCache,
    DocError,
    Document,
};
use super::ingest::META_FILE;
use super::{
    CACHE_MAX_IDLE,
    DOC_CACHE_CAPACITY,
    TOUCH_INTERVAL,
};
use crate::domain::{
    DomainError,
    DomainResult,
    LogCacheStats,
    LogDocumentMeta,
    LogSection,
    LogStyle,
    PipelineStatus,
    RunLogTree,
};

const LAYOUT_DIR: &str = "v1";
const TMP_DIR: &str = "tmp";
const DOCS_DIR: &str = "docs";
const TOUCH_FILE: &str = ".touch";
const TREE_FILE: &str = "tree.json";
const TREE_TMP_FILE: &str = "tree.json.tmp";
pub(super) const STORED_FORMAT: u32 = 4;

type RunId = (i64, String);
type TrimScope = (HashSet<RunId>, Option<(RunId, String)>);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocKey {
    pub provider_id: i64,
    pub pipeline_id: String,
    pub run_number: i64,
    pub node_id: String,
}

pub(super) fn short_hex(digest: &[u8]) -> String {
    let mut head = [0u8; 16];
    head.copy_from_slice(&digest[..16]);
    format!("{:032x}", u128::from_be_bytes(head))
}

fn hex_key(input: &str) -> String {
    short_hex(&Sha256::digest(input.as_bytes()))
}

fn run_key_of(pipeline_id: &str, run_number: i64) -> String {
    hex_key(&format!("{pipeline_id}\n{run_number}"))
}

impl DocKey {
    pub fn run_key(&self) -> String {
        run_key_of(&self.pipeline_id, self.run_number)
    }

    pub fn run_id(&self) -> (i64, String) {
        (self.provider_id, self.run_key())
    }

    fn doc_key(&self) -> String {
        hex_key(&self.node_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDocMeta {
    pub format: u32,
    pub key: DocKey,
    pub revision: i64,
    pub complete: bool,
    pub truncated: bool,
    pub line_count: u32,
    pub text_bytes: u64,
    pub disk_bytes: u64,
    pub first_timestamp_ms: Option<i64>,
    pub styles: Vec<LogStyle>,
    pub sections: Vec<LogSection>,
    pub error_lines: Vec<u32>,
    pub warning_lines: Vec<u32>,
    pub error_total: u32,
    pub warning_total: u32,
    pub focus_line: Option<u32>,
    pub updated_at_ms: i64,
    pub stable_lines: u32,
    pub previous_revision: Option<i64>,
}

impl StoredDocMeta {
    pub fn to_meta(&self) -> LogDocumentMeta {
        LogDocumentMeta {
            node_id: self.key.node_id.clone(),
            revision: self.revision,
            complete: self.complete,
            truncated: self.truncated,
            line_count: self.line_count,
            text_bytes: self.text_bytes,
            first_timestamp_ms: self.first_timestamp_ms,
            styles: self.styles.clone(),
            sections: self.sections.clone(),
            error_lines: self.error_lines.clone(),
            warning_lines: self.warning_lines.clone(),
            error_total: self.error_total,
            warning_total: self.warning_total,
            focus_line: self.focus_line,
            updated_at_ms: self.updated_at_ms,
            stable_lines: self.stable_lines,
            previous_revision: self.previous_revision,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTree {
    pub run_status: PipelineStatus,
    pub run_concluded_at: Option<DateTime<Utc>>,
    pub tree: RunLogTree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheEpoch {
    pub global: u64,
    pub provider: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Published,
    StaleEpoch,
    StaleBase,
}

struct RevisionState {
    dir: PathBuf,
    bytes: u64,
    readers: AtomicU32,
    doomed: AtomicBool,
}

pub struct ReadLease {
    state: Arc<RevisionState>,
    deletes: Arc<Mutex<Vec<PathBuf>>>,
}

impl Drop for ReadLease {
    fn drop(&mut self) {
        if self.state.readers.fetch_sub(1, Ordering::SeqCst) == 1
            && self.state.doomed.load(Ordering::SeqCst)
        {
            lock(&self.deletes).push(self.state.dir.clone());
        }
    }
}

struct DocEntry {
    current: Option<i64>,
    revisions: BTreeMap<i64, Arc<RevisionState>>,
    last_access: SystemTime,
}

impl DocEntry {
    fn new(last_access: SystemTime) -> Self {
        Self {
            current: None,
            revisions: BTreeMap::new(),
            last_access,
        }
    }

    fn bytes(&self) -> u64 {
        self.revisions.values().map(|r| r.bytes).sum()
    }

    fn leased(&self) -> bool {
        self.revisions
            .values()
            .any(|r| r.readers.load(Ordering::SeqCst) > 0)
    }
}

struct RunEntry {
    dir: PathBuf,
    tree_bytes: u64,
    last_access: SystemTime,
    last_touch: SystemTime,
    docs: HashMap<String, DocEntry>,
}

impl RunEntry {
    fn bytes(&self) -> u64 {
        self.tree_bytes
            + self
                .docs
                .values()
                .flat_map(|d| d.revisions.values())
                .map(|r| r.bytes)
                .sum::<u64>()
    }

    fn leased(&self) -> bool {
        self.docs
            .values()
            .flat_map(|d| d.revisions.values())
            .any(|r| r.readers.load(Ordering::SeqCst) > 0)
    }
}

#[derive(Default)]
struct Registry {
    global_epoch: u64,
    provider_epochs: HashMap<i64, u64>,
    last_revision: i64,
    runs: HashMap<RunId, RunEntry>,
    retired: Vec<Arc<RevisionState>>,
    last_idle_sweep: Option<SystemTime>,
}

impl Registry {
    fn epoch(&self, provider_id: i64) -> CacheEpoch {
        CacheEpoch {
            global: self.global_epoch,
            provider: self.provider_epochs.get(&provider_id).copied().unwrap_or(0),
        }
    }

    fn total_bytes(&self) -> u64 {
        self.runs.values().map(RunEntry::bytes).sum::<u64>()
            + self.retired.iter().map(|r| r.bytes).sum::<u64>()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn retire(
    retired: &mut Vec<Arc<RevisionState>>, deletes: &Mutex<Vec<PathBuf>>, state: Arc<RevisionState>,
) {
    state.doomed.store(true, Ordering::SeqCst);
    if state.readers.load(Ordering::SeqCst) == 0 {
        lock(deletes).push(state.dir.clone());
    }
    retired.push(state);
}

fn remove_file_quiet(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("Couldn't remove cached log file: {e}"),
    }
}

fn remove_run(reg: &mut Registry, deletes: &Mutex<Vec<PathBuf>>, id: &RunId) -> Vec<PathBuf> {
    let Some(run) = reg.runs.remove(id) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for doc in run.docs.into_values() {
        for state in doc.revisions.into_values() {
            dirs.push(state.dir.clone());
            retire(&mut reg.retired, deletes, state);
        }
    }
    remove_file_quiet(&run.dir.join(TREE_FILE));
    remove_file_quiet(&run.dir.join(TOUCH_FILE));
    dirs
}

fn remove_doc(
    reg: &mut Registry, deletes: &Mutex<Vec<PathBuf>>, id: &RunId, doc_key: &str,
) -> Vec<PathBuf> {
    let Some(run) = reg.runs.get_mut(id) else {
        return Vec::new();
    };
    let Some(doc) = run.docs.remove(doc_key) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for state in doc.revisions.into_values() {
        dirs.push(state.dir.clone());
        retire(&mut reg.retired, deletes, state);
    }
    if run.docs.is_empty() && run.tree_bytes == 0 {
        dirs.extend(remove_run(reg, deletes, id));
    }
    dirs
}

fn dir_size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

fn io_error(context: &str, error: &io::Error) -> DomainError {
    DomainError::InternalError(format!("{context}: {error}"))
}

fn join_error(error: tokio::task::JoinError) -> DomainError {
    DomainError::InternalError(format!("Log cache task failed: {error}"))
}

fn touch_file(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?
        .set_modified(SystemTime::now())
}

static STORES: LazyLock<tokio::sync::Mutex<HashMap<PathBuf, Weak<LogStore>>>> =
    LazyLock::new(Default::default);

pub struct LogStore {
    root: PathBuf,
    budget: u64,
    touch_interval: Duration,
    registry: Mutex<Registry>,
    deletes: Arc<Mutex<Vec<PathBuf>>>,
    documents: Mutex<Vec<(PathBuf, Arc<Document>)>>,
    blocks: BlockCache,
}

impl LogStore {
    pub async fn open(root: PathBuf, budget: u64) -> DomainResult<Arc<LogStore>> {
        Self::open_with(root, budget, TOUCH_INTERVAL).await
    }

    async fn open_with(
        root: PathBuf, budget: u64, touch_interval: Duration,
    ) -> DomainResult<Arc<LogStore>> {
        let open_error = |e: io::Error| io_error("Couldn't open the log cache", &e);
        let root = tokio::task::spawn_blocking(move || {
            fs::create_dir_all(&root)?;
            fs::canonicalize(&root)
        })
        .await
        .map_err(join_error)?
        .map_err(open_error)?;

        let mut stores = STORES.lock().await;
        if let Some(store) = stores.get(&root).and_then(Weak::upgrade) {
            return Ok(store);
        }
        let scan_root = root.clone();
        let registry = tokio::task::spawn_blocking(move || scan(&scan_root, SystemTime::now()))
            .await
            .map_err(join_error)?
            .map_err(open_error)?;
        let store = Arc::new(LogStore {
            root: root.clone(),
            budget,
            touch_interval,
            registry: Mutex::new(registry),
            deletes: Arc::default(),
            documents: Mutex::new(Vec::new()),
            blocks: BlockCache::default(),
        });
        stores.retain(|_, weak| weak.strong_count() > 0);
        stores.insert(root, Arc::downgrade(&store));
        Ok(store)
    }

    fn base_dir(&self) -> PathBuf {
        self.root.join(LAYOUT_DIR)
    }

    fn tmp_root(&self) -> PathBuf {
        self.base_dir().join(TMP_DIR)
    }

    fn run_dir(&self, provider_id: i64, run_key: &str) -> PathBuf {
        self.base_dir().join(provider_id.to_string()).join(run_key)
    }

    pub fn block_cache(&self) -> &BlockCache {
        &self.blocks
    }

    pub fn epoch(&self, provider_id: i64) -> CacheEpoch {
        lock(&self.registry).epoch(provider_id)
    }

    pub fn allocate_revision(&self) -> i64 {
        self.allocate_at(Utc::now().timestamp_millis())
    }

    fn allocate_at(&self, now_ms: i64) -> i64 {
        let mut reg = lock(&self.registry);
        let revision = now_ms.max(reg.last_revision + 1);
        reg.last_revision = revision;
        revision
    }

    pub async fn tmp_dir(&self) -> DomainResult<PathBuf> {
        let dir = self
            .tmp_root()
            .join(format!("{:032x}", rand::random::<u128>()));
        tokio::task::spawn_blocking(move || fs::create_dir_all(&dir).map(|()| dir))
            .await
            .map_err(join_error)?
            .map_err(|e| io_error("Couldn't prepare the log download", &e))
    }

    pub async fn discard(&self, tmp: PathBuf) {
        if !tmp.starts_with(self.tmp_root()) {
            return;
        }
        let removed = tokio::task::spawn_blocking(move || match fs::remove_dir_all(&tmp) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        })
        .await;
        match removed {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("Couldn't remove a temporary log download: {e}"),
            Err(e) => tracing::warn!("Couldn't remove a temporary log download: {e}"),
        }
    }

    pub fn current(&self, key: &DocKey) -> Option<i64> {
        lock(&self.registry)
            .runs
            .get(&key.run_id())
            .and_then(|run| run.docs.get(&key.doc_key()))
            .and_then(|doc| doc.current)
    }

    pub async fn lease(
        self: &Arc<Self>, key: &DocKey, revision: i64,
    ) -> DomainResult<Option<(Arc<Document>, ReadLease)>> {
        let store = self.clone();
        let key = key.clone();
        tokio::task::spawn_blocking(move || store.lease_blocking(&key, revision))
            .await
            .map_err(join_error)?
    }

    fn lease_blocking(
        &self, key: &DocKey, revision: i64,
    ) -> DomainResult<Option<(Arc<Document>, ReadLease)>> {
        let now = SystemTime::now();
        let (lease, touch) = {
            let mut reg = lock(&self.registry);
            let Some(run) = reg.runs.get_mut(&key.run_id()) else {
                return Ok(None);
            };
            let Some(doc) = run.docs.get_mut(&key.doc_key()) else {
                return Ok(None);
            };
            let Some(state) = doc
                .revisions
                .get(&revision)
                .filter(|state| !state.doomed.load(Ordering::SeqCst))
                .cloned()
            else {
                return Ok(None);
            };
            doc.last_access = now;
            state.readers.fetch_add(1, Ordering::SeqCst);
            let lease = ReadLease {
                state,
                deletes: self.deletes.clone(),
            };
            run.last_access = now;
            let stale =
                now.duration_since(run.last_touch).unwrap_or_default() >= self.touch_interval;
            let touch = stale.then(|| {
                run.last_touch = now;
                run.dir.join(TOUCH_FILE)
            });
            (lease, touch)
        };

        let document = match self.load_document(&lease.state.dir) {
            Ok(document) => document,
            Err(error @ (DocError::Corrupt | DocError::Io(_)))
                if matches!(error, DocError::Corrupt) || error.is_missing() =>
            {
                tracing::warn!("Dropping an unreadable cached log: {error}");
                drop(lease);
                self.mark_corrupt(key, revision);
                return Ok(None);
            }
            Err(error) => {
                return Err(DomainError::InternalError(format!(
                    "Couldn't read the log: {error}"
                )))
            }
        };
        if let Some(path) = touch {
            if let Err(e) = touch_file(&path) {
                tracing::warn!("Couldn't update the log cache access time: {e}");
            }
        }
        Ok(Some((document, lease)))
    }

    fn load_document(&self, dir: &Path) -> Result<Arc<Document>, DocError> {
        {
            let mut documents = lock(&self.documents);
            if let Some(position) = documents.iter().position(|(path, _)| path == dir) {
                let entry = documents.remove(position);
                let document = entry.1.clone();
                documents.push(entry);
                return Ok(document);
            }
        }
        let document = Arc::new(Document::open(dir)?);
        let mut documents = lock(&self.documents);
        if !documents.iter().any(|(path, _)| path == dir) {
            if documents.len() >= DOC_CACHE_CAPACITY {
                documents.remove(0);
            }
            documents.push((dir.to_path_buf(), document.clone()));
        }
        Ok(document)
    }

    fn forget(&self, dirs: &[PathBuf]) {
        if dirs.is_empty() {
            return;
        }
        lock(&self.documents).retain(|(path, _)| !dirs.contains(path));
        for dir in dirs {
            self.blocks.forget(dir);
        }
    }

    pub fn mark_corrupt(&self, key: &DocKey, revision: i64) {
        let doomed = {
            let mut reg = lock(&self.registry);
            let state = reg
                .runs
                .get_mut(&key.run_id())
                .and_then(|run| run.docs.get_mut(&key.doc_key()))
                .and_then(|doc| {
                    if doc.current == Some(revision) {
                        doc.current = None;
                    }
                    doc.revisions.remove(&revision)
                });
            state.map(|state| {
                let dir = state.dir.clone();
                retire(&mut reg.retired, &self.deletes, state);
                dir
            })
        };
        if let Some(dir) = doomed {
            self.forget(&[dir]);
        }
    }

    pub async fn base(self: &Arc<Self>, key: &DocKey) -> DomainResult<Option<Base>> {
        let Some(revision) = self.current(key) else {
            return Ok(None);
        };
        let Some((document, lease)) = self.lease(key, revision).await? else {
            return Ok(None);
        };
        let loaded = Arc::clone(&document);
        let checkpoint = tokio::task::spawn_blocking(move || Checkpoint::load(&loaded))
            .await
            .map_err(join_error)?;

        Ok(Some(Base {
            revision,
            document,
            _lease: lease,
            checkpoint,
        }))
    }

    pub async fn publish(
        self: &Arc<Self>, key: &DocKey, tmp: &Path, revision: i64, epoch: CacheEpoch,
        expected_current: Option<i64>, busy: HashSet<(i64, String)>,
    ) -> DomainResult<PublishOutcome> {
        if let Err(e) = self.sweep().await {
            tracing::warn!("{e}");
        }
        let store = self.clone();
        let (key, tmp) = (key.clone(), tmp.to_path_buf());
        let (outcome, dirs) = tokio::task::spawn_blocking(move || {
            store.publish_blocking(&key, &tmp, revision, epoch, expected_current, &busy)
        })
        .await
        .map_err(join_error)??;
        self.forget(&dirs);
        Ok(outcome)
    }

    fn publish_blocking(
        &self, key: &DocKey, tmp: &Path, revision: i64, epoch: CacheEpoch,
        expected_current: Option<i64>, busy: &HashSet<RunId>,
    ) -> DomainResult<(PublishOutcome, Vec<PathBuf>)> {
        let save_error = |e: io::Error| io_error("Couldn't save the log", &e);
        let mut guard = lock(&self.registry);
        let reg = &mut *guard;
        if reg.epoch(key.provider_id) != epoch {
            return Ok((PublishOutcome::StaleEpoch, Vec::new()));
        }
        let current = reg
            .runs
            .get(&key.run_id())
            .and_then(|run| run.docs.get(&key.doc_key()))
            .and_then(|doc| doc.current);
        if current != expected_current {
            return Ok((PublishOutcome::StaleBase, Vec::new()));
        }
        let run_id = key.run_id();
        let doc_key = key.doc_key();
        let run_dir = self.run_dir(key.provider_id, &run_id.1);
        let doc_dir = run_dir.join(DOCS_DIR).join(&doc_key);
        let target = doc_dir.join(revision.to_string());
        fs::create_dir_all(&doc_dir).map_err(save_error)?;
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(run_dir.join(TOUCH_FILE))
            .map_err(save_error)?;
        if target.exists() {
            return Err(save_error(io::Error::from(io::ErrorKind::AlreadyExists)));
        }
        fs::rename(tmp, &target).map_err(save_error)?;

        let now = SystemTime::now();
        let state = Arc::new(RevisionState {
            bytes: dir_size(&target),
            dir: target,
            readers: AtomicU32::new(0),
            doomed: AtomicBool::new(false),
        });
        let run = reg.runs.entry(run_id.clone()).or_insert_with(|| RunEntry {
            dir: run_dir,
            tree_bytes: 0,
            last_access: now,
            last_touch: now,
            docs: HashMap::new(),
        });
        run.last_access = now;
        let doc = run
            .docs
            .entry(doc_key.clone())
            .or_insert_with(|| DocEntry::new(now));
        doc.last_access = now;
        let previous = doc
            .current
            .take()
            .and_then(|current| doc.revisions.remove(&current));
        doc.revisions.insert(revision, state);
        doc.current = Some(revision);
        let mut dirs = Vec::new();
        if let Some(previous) = previous {
            dirs.push(previous.dir.clone());
            retire(&mut reg.retired, &self.deletes, previous);
        }
        reg.last_revision = reg.last_revision.max(revision);
        dirs.extend(self.evict(reg, busy, Some((&run_id, &doc_key))));
        Ok((PublishOutcome::Published, dirs))
    }

    fn evict(
        &self, reg: &mut Registry, busy: &HashSet<RunId>, keep: Option<(&RunId, &str)>,
    ) -> Vec<PathBuf> {
        let mut total = reg.total_bytes();
        if total <= self.budget {
            return Vec::new();
        }
        let target = self.budget / 10 * 9;
        let keep_run = keep.map(|(run, _)| run);
        let mut runs: Vec<(SystemTime, RunId, u64)> = reg
            .runs
            .iter()
            .filter(|(id, run)| Some(*id) != keep_run && !busy.contains(*id) && !run.leased())
            .map(|(id, run)| (run.last_access, id.clone(), run.bytes()))
            .collect();
        runs.sort_by_key(|candidate| candidate.0);
        let mut dirs = Vec::new();
        for (_, id, bytes) in runs {
            if total <= target {
                return dirs;
            }
            total = total.saturating_sub(bytes);
            dirs.extend(remove_run(reg, &self.deletes, &id));
        }

        let mut docs: Vec<(SystemTime, RunId, String, u64)> = Vec::new();
        for (id, run) in &reg.runs {
            if Some(id) != keep_run && busy.contains(id) {
                continue;
            }
            for (doc_key, doc) in &run.docs {
                let protected = keep.is_some_and(|(r, d)| r == id && d == doc_key.as_str());
                if !protected && !doc.leased() {
                    docs.push((doc.last_access, id.clone(), doc_key.clone(), doc.bytes()));
                }
            }
        }
        docs.sort_by_key(|candidate| candidate.0);
        for (_, id, doc_key, bytes) in docs {
            if total <= target {
                break;
            }
            total = total.saturating_sub(bytes);
            dirs.extend(remove_doc(reg, &self.deletes, &id, &doc_key));
        }
        dirs
    }

    pub async fn read_tree(
        self: &Arc<Self>, provider_id: i64, pipeline_id: &str, run_number: i64,
    ) -> Option<StoredTree> {
        let path = self
            .run_dir(provider_id, &run_key_of(pipeline_id, run_number))
            .join(TREE_FILE);
        tokio::task::spawn_blocking(move || serde_json::from_slice(&fs::read(path).ok()?).ok())
            .await
            .ok()
            .flatten()
    }

    pub async fn write_tree(
        self: &Arc<Self>, provider_id: i64, pipeline_id: &str, run_number: i64, tree: &StoredTree,
        epoch: CacheEpoch,
    ) -> DomainResult<()> {
        let json = serde_json::to_vec(tree)
            .map_err(|e| DomainError::InternalError(format!("Couldn't save the log tree: {e}")))?;
        let store = self.clone();
        let run_id = (provider_id, run_key_of(pipeline_id, run_number));
        tokio::task::spawn_blocking(move || store.write_tree_blocking(run_id, &json, epoch))
            .await
            .map_err(join_error)?
    }

    fn write_tree_blocking(
        &self, run_id: RunId, json: &[u8], epoch: CacheEpoch,
    ) -> DomainResult<()> {
        let save_error = |e: io::Error| io_error("Couldn't save the log tree", &e);
        let mut reg = lock(&self.registry);
        if reg.epoch(run_id.0) != epoch {
            return Ok(());
        }
        let run_dir = self.run_dir(run_id.0, &run_id.1);
        fs::create_dir_all(&run_dir).map_err(save_error)?;
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(run_dir.join(TOUCH_FILE))
            .map_err(save_error)?;
        let tmp = run_dir.join(TREE_TMP_FILE);
        fs::write(&tmp, json).map_err(save_error)?;
        fs::rename(&tmp, run_dir.join(TREE_FILE)).map_err(save_error)?;
        let now = SystemTime::now();
        let run = reg.runs.entry(run_id).or_insert_with(|| RunEntry {
            dir: run_dir,
            tree_bytes: 0,
            last_access: now,
            last_touch: now,
            docs: HashMap::new(),
        });
        run.tree_bytes = json.len() as u64;
        Ok(())
    }

    pub async fn purge_provider(self: &Arc<Self>, provider_id: i64) -> DomainResult<()> {
        self.invalidate(Some(provider_id)).await
    }

    pub async fn clear(self: &Arc<Self>) -> DomainResult<()> {
        self.invalidate(None).await
    }

    async fn invalidate(self: &Arc<Self>, provider_id: Option<i64>) -> DomainResult<()> {
        if let Err(e) = self.sweep().await {
            tracing::warn!("{e}");
        }
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let dirs = {
                let mut guard = lock(&store.registry);
                let reg = &mut *guard;
                match provider_id {
                    Some(id) => *reg.provider_epochs.entry(id).or_insert(0) += 1,
                    None => reg.global_epoch += 1,
                }
                let ids: Vec<RunId> = reg
                    .runs
                    .keys()
                    .filter(|(provider, _)| provider_id.is_none_or(|id| *provider == id))
                    .cloned()
                    .collect();
                ids.iter()
                    .flat_map(|id| remove_run(reg, &store.deletes, id))
                    .collect::<Vec<_>>()
            };
            store.forget(&dirs);
        })
        .await
        .map_err(join_error)?;
        self.sweep().await
    }

    pub async fn stats(self: &Arc<Self>) -> LogCacheStats {
        if let Err(e) = self.sweep().await {
            tracing::warn!("{e}");
        }
        let reg = lock(&self.registry);
        LogCacheStats {
            bytes: reg.total_bytes(),
            runs: reg.runs.len() as u64,
        }
    }

    pub(crate) async fn sweep(self: &Arc<Self>) -> DomainResult<()> {
        self.run_sweep(None).await
    }

    pub async fn sweep_with(
        self: &Arc<Self>, busy: HashSet<(i64, String)>, keep: Option<&DocKey>,
    ) -> DomainResult<()> {
        let keep = keep.map(|key| (key.run_id(), key.doc_key()));
        self.run_sweep(Some((busy, keep))).await
    }

    async fn run_sweep(self: &Arc<Self>, trim: Option<TrimScope>) -> DomainResult<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.sweep_blocking(trim.as_ref()))
            .await
            .map_err(join_error)?
    }

    fn sweep_blocking(&self, trim: Option<&TrimScope>) -> DomainResult<()> {
        let first = self.drain_deletes();
        let Some((busy, keep)) = trim else {
            return first;
        };
        let keep = keep.as_ref().map(|(run, doc)| (run, doc.as_str()));
        let dirs = self.trim(busy, keep);
        self.forget(&dirs);
        let second = self.drain_deletes();
        first.and(second)
    }

    fn trim(&self, busy: &HashSet<RunId>, keep: Option<(&RunId, &str)>) -> Vec<PathBuf> {
        let now = SystemTime::now();
        let mut guard = lock(&self.registry);
        let reg = &mut *guard;
        let mut dirs = Vec::new();
        let due = reg
            .last_idle_sweep
            .is_none_or(|last| now.duration_since(last).unwrap_or_default() >= self.touch_interval);
        if due {
            reg.last_idle_sweep = Some(now);
            let expired: Vec<RunId> = reg
                .runs
                .iter()
                .filter(|(id, run)| {
                    !busy.contains(*id)
                        && !run.leased()
                        && now.duration_since(run.last_access).unwrap_or_default() > CACHE_MAX_IDLE
                })
                .map(|(id, _)| id.clone())
                .collect();
            for id in expired {
                dirs.extend(remove_run(reg, &self.deletes, &id));
            }
        }
        dirs.extend(self.evict(reg, busy, keep));
        dirs
    }

    fn drain_deletes(&self) -> DomainResult<()> {
        let pending = std::mem::take(&mut *lock(&self.deletes));
        let mut removed = Vec::new();
        let mut failed = Vec::new();
        let mut first_error: Option<io::Error> = None;
        for dir in pending {
            match fs::remove_dir_all(&dir) {
                Ok(()) => removed.push(dir),
                Err(e) if e.kind() == io::ErrorKind::NotFound => removed.push(dir),
                Err(e) => {
                    failed.push(dir);
                    first_error.get_or_insert(e);
                }
            }
        }
        if !failed.is_empty() {
            lock(&self.deletes).extend(failed);
        }
        {
            let mut reg = lock(&self.registry);
            reg.retired.retain(|state| state.dir.exists());
            for dir in &removed {
                self.prune_ancestors(&reg, dir);
            }
        }
        match first_error {
            Some(e) => Err(io_error("Couldn't delete cached logs", &e)),
            None => Ok(()),
        }
    }

    fn prune_ancestors(&self, reg: &Registry, revision_dir: &Path) {
        let Ok(relative) = revision_dir.strip_prefix(&self.root) else {
            return;
        };
        let parts: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let [_, provider, run_key, _, doc_key, _] = parts.as_slice() else {
            return;
        };
        let Ok(provider_id) = provider.parse::<i64>() else {
            return;
        };
        let run = reg.runs.get(&(provider_id, run_key.clone()));
        let referenced = [
            run.is_some_and(|run| run.docs.contains_key(doc_key)),
            run.is_some_and(|run| !run.docs.is_empty()),
            run.is_some(),
            reg.runs.keys().any(|(id, _)| *id == provider_id),
        ];
        let mut dir = revision_dir.parent();
        for referenced in referenced {
            let Some(current) = dir else {
                return;
            };
            if referenced || fs::remove_dir(current).is_err() {
                return;
            }
            dir = current.parent();
        }
    }
}

fn valid_meta(dir: &Path) -> bool {
    fs::read(dir.join(META_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredDocMeta>(&bytes).ok())
        .is_some_and(|meta| meta.format == STORED_FORMAT)
}

fn remove_path(path: &Path) {
    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    if let Err(e) = result {
        if e.kind() != io::ErrorKind::NotFound {
            tracing::warn!("Couldn't remove a stale log cache entry: {e}");
        }
    }
}

fn scan(root: &Path, now: SystemTime) -> io::Result<Registry> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() != LAYOUT_DIR {
            remove_path(&entry.path());
        }
    }
    let base = root.join(LAYOUT_DIR);
    fs::create_dir_all(&base)?;
    remove_path(&base.join(TMP_DIR));

    let mut reg = Registry::default();
    for provider in fs::read_dir(&base)? {
        let provider = provider?;
        let provider_dir = provider.path();
        let Some(provider_id) = provider
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<i64>().ok())
        else {
            remove_path(&provider_dir);
            continue;
        };
        if !provider_dir.is_dir() {
            remove_path(&provider_dir);
            continue;
        }
        for run in fs::read_dir(&provider_dir)? {
            let run = run?;
            let run_dir = run.path();
            let Some(run_key) = run.file_name().to_str().map(str::to_owned) else {
                remove_path(&run_dir);
                continue;
            };
            match scan_run(&run_dir, now) {
                Some((entry, newest)) => {
                    reg.last_revision = reg.last_revision.max(newest);
                    reg.runs.insert((provider_id, run_key), entry);
                }
                None => remove_path(&run_dir),
            }
        }
        let _ = fs::remove_dir(&provider_dir);
    }
    Ok(reg)
}

fn scan_run(run_dir: &Path, now: SystemTime) -> Option<(RunEntry, i64)> {
    if !run_dir.is_dir() {
        return None;
    }
    let touch = fs::metadata(run_dir.join(TOUCH_FILE))
        .and_then(|m| m.modified())
        .or_else(|_| fs::metadata(run_dir).and_then(|m| m.modified()))
        .unwrap_or(now);
    if now.duration_since(touch).unwrap_or_default() > CACHE_MAX_IDLE {
        return None;
    }
    remove_file_quiet(&run_dir.join(TREE_TMP_FILE));
    let tree_bytes = fs::metadata(run_dir.join(TREE_FILE)).map_or(0, |m| m.len());

    let mut docs = HashMap::new();
    let mut newest = 0;
    let docs_dir = run_dir.join(DOCS_DIR);
    for doc in fs::read_dir(&docs_dir).into_iter().flatten().flatten() {
        let doc_dir = doc.path();
        let Some(doc_key) = doc.file_name().to_str().map(str::to_owned) else {
            remove_path(&doc_dir);
            continue;
        };
        let mut revisions: Vec<(i64, PathBuf)> = Vec::new();
        for revision in fs::read_dir(&doc_dir).into_iter().flatten().flatten() {
            match revision
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<i64>().ok())
            {
                Some(number) if revision.path().is_dir() => {
                    revisions.push((number, revision.path()))
                }
                _ => remove_path(&revision.path()),
            }
        }
        revisions.sort_by_key(|(number, _)| std::cmp::Reverse(*number));
        let keep = revisions.iter().position(|(_, dir)| valid_meta(dir));
        let mut entry = DocEntry::new(touch);
        for (position, (number, dir)) in revisions.into_iter().enumerate() {
            if Some(position) == keep {
                let state = RevisionState {
                    bytes: dir_size(&dir),
                    dir,
                    readers: AtomicU32::new(0),
                    doomed: AtomicBool::new(false),
                };
                entry.current = Some(number);
                entry.revisions.insert(number, Arc::new(state));
                newest = newest.max(number);
            } else {
                remove_path(&dir);
            }
        }
        if entry.current.is_some() {
            docs.insert(doc_key, entry);
        } else {
            remove_path(&doc_dir);
        }
    }
    if docs.is_empty() && tree_bytes == 0 {
        return None;
    }
    let run = RunEntry {
        dir: run_dir.to_path_buf(),
        tree_bytes,
        last_access: touch,
        last_touch: touch,
        docs,
    };
    Some((run, newest))
}

#[cfg(test)]
mod tests {
    use std::fs::{
        File,
        FileTimes,
    };

    use tempfile::TempDir;

    use super::super::checkpoint::{
        CHECKPOINT_FILE,
        TAIL_FILE,
    };
    use super::super::ingest::INDEX_FILE;
    use super::super::testing::{
        doc_key,
        feed,
        finish,
        ingester,
    };
    use super::super::BlockLimits;
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
            let blocks =
                |revision| fs::metadata(rev_dir(&store, &key, revision).join("blocks.bin"));
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
            run_status: PipelineStatus::Success,
            run_concluded_at: None,
            tree: RunLogTree {
                nodes: Vec::new(),
                default_node_id: None,
                complete: true,
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
}
