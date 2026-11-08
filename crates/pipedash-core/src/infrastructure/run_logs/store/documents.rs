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
use std::sync::atomic::{
    AtomicBool,
    AtomicU32,
    Ordering,
};
use std::sync::Arc;
use std::time::SystemTime;

use super::registry::{
    lock,
    retire,
    DocEntry,
    RevisionState,
    RunEntry,
};
use super::{
    dir_size,
    io_error,
    join_error,
    touch_file,
    CacheEpoch,
    DocKey,
    LogStore,
    PublishOutcome,
    ReadLease,
    RunId,
    DOCS_DIR,
    TOUCH_FILE,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::run_logs::document::{
    DocError,
    Document,
};
use crate::infrastructure::run_logs::DOC_CACHE_CAPACITY;

impl LogStore {
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

    pub(super) fn forget(&self, dirs: &[PathBuf]) {
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
}
