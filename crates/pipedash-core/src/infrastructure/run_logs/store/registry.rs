use std::collections::{
    BTreeMap,
    HashMap,
};
use std::path::PathBuf;
use std::sync::atomic::{
    AtomicBool,
    AtomicU32,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::SystemTime;

use super::{
    remove_file_quiet,
    CacheEpoch,
    RunId,
    TOUCH_FILE,
    TREE_FILE,
};

pub(super) struct RevisionState {
    pub(super) dir: PathBuf,
    pub(super) bytes: u64,
    pub(super) readers: AtomicU32,
    pub(super) doomed: AtomicBool,
}

pub(super) struct DocEntry {
    pub(super) current: Option<i64>,
    pub(super) revisions: BTreeMap<i64, Arc<RevisionState>>,
    pub(super) last_access: SystemTime,
}

impl DocEntry {
    pub(super) fn new(last_access: SystemTime) -> Self {
        Self {
            current: None,
            revisions: BTreeMap::new(),
            last_access,
        }
    }

    pub(super) fn bytes(&self) -> u64 {
        self.revisions.values().map(|r| r.bytes).sum()
    }

    pub(super) fn leased(&self) -> bool {
        self.revisions
            .values()
            .any(|r| r.readers.load(Ordering::SeqCst) > 0)
    }
}

pub(super) struct RunEntry {
    pub(super) dir: PathBuf,
    pub(super) tree_bytes: u64,
    pub(super) last_access: SystemTime,
    pub(super) last_touch: SystemTime,
    pub(super) docs: HashMap<String, DocEntry>,
}

impl RunEntry {
    pub(super) fn bytes(&self) -> u64 {
        self.tree_bytes
            + self
                .docs
                .values()
                .flat_map(|d| d.revisions.values())
                .map(|r| r.bytes)
                .sum::<u64>()
    }

    pub(super) fn leased(&self) -> bool {
        self.docs
            .values()
            .flat_map(|d| d.revisions.values())
            .any(|r| r.readers.load(Ordering::SeqCst) > 0)
    }
}

#[derive(Default)]
pub(super) struct Registry {
    pub(super) global_epoch: u64,
    pub(super) provider_epochs: HashMap<i64, u64>,
    pub(super) last_revision: i64,
    pub(super) runs: HashMap<RunId, RunEntry>,
    pub(super) retired: Vec<Arc<RevisionState>>,
    pub(super) last_idle_sweep: Option<SystemTime>,
}

impl Registry {
    pub(super) fn epoch(&self, provider_id: i64) -> CacheEpoch {
        CacheEpoch {
            global: self.global_epoch,
            provider: self.provider_epochs.get(&provider_id).copied().unwrap_or(0),
        }
    }

    pub(super) fn total_bytes(&self) -> u64 {
        self.runs.values().map(RunEntry::bytes).sum::<u64>()
            + self.retired.iter().map(|r| r.bytes).sum::<u64>()
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn retire(
    retired: &mut Vec<Arc<RevisionState>>, deletes: &Mutex<Vec<PathBuf>>, state: Arc<RevisionState>,
) {
    state.doomed.store(true, Ordering::SeqCst);
    if state.readers.load(Ordering::SeqCst) == 0 {
        lock(deletes).push(state.dir.clone());
    }
    retired.push(state);
}

pub(super) fn remove_run(
    reg: &mut Registry, deletes: &Mutex<Vec<PathBuf>>, id: &RunId,
) -> Vec<PathBuf> {
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

pub(super) fn remove_doc(
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
