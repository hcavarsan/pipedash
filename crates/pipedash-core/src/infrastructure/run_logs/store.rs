use std::collections::{
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
use std::sync::atomic::Ordering;
use std::sync::{
    Arc,
    LazyLock,
    Mutex,
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

use self::registry::{
    lock,
    Registry,
    RevisionState,
};
use self::scan::scan;
use super::checkpoint::{
    Base,
    Checkpoint,
};
use super::document::cache::BlockCache;
use super::document::Document;
use super::TOUCH_INTERVAL;
use crate::domain::{
    DomainError,
    DomainResult,
    LogDocumentMeta,
    LogSection,
    LogStyle,
    PipelineStatus,
    RunLogTree,
};

mod documents;
mod eviction;
mod registry;
mod scan;
mod transfer;
mod tree;

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
    #[serde(default)]
    pub log_tree_version: u32,
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

fn remove_file_quiet(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("Couldn't remove cached log file: {e}"),
    }
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
}

#[cfg(test)]
mod tests;
