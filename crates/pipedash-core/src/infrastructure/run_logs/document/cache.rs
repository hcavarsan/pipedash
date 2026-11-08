use std::collections::{
    BTreeMap,
    HashMap,
};
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};

use crate::infrastructure::run_logs::ansi::Span;
use crate::infrastructure::run_logs::block::{
    DecodedBlock,
    LineMeta,
};
use crate::infrastructure::run_logs::BLOCK_CACHE_BUDGET_BYTES;

pub(super) type CacheKey = (PathBuf, u32);

struct CacheEntry {
    block: Arc<DecodedBlock>,
    size: usize,
    tick: u64,
}

#[derive(Default)]
struct CacheInner {
    map: HashMap<CacheKey, CacheEntry>,
    order: BTreeMap<u64, CacheKey>,
    tick: u64,
    total: usize,
}

pub struct BlockCache {
    inner: Mutex<CacheInner>,
    budget: usize,
}

impl Default for BlockCache {
    fn default() -> Self {
        Self::new(BLOCK_CACHE_BUDGET_BYTES)
    }
}

impl BlockCache {
    pub fn new(budget: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner::default()),
            budget,
        }
    }

    pub(super) fn get(&self, key: &CacheKey) -> Option<Arc<DecodedBlock>> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.tick += 1;
        let tick = inner.tick;
        let entry = inner.map.get_mut(key)?;
        let old = std::mem::replace(&mut entry.tick, tick);
        let block = entry.block.clone();
        inner.order.remove(&old);
        inner.order.insert(tick, key.clone());
        Some(block)
    }

    pub(super) fn insert(&self, key: CacheKey, block: Arc<DecodedBlock>) {
        let size = block.text.len()
            + block.lines.len() * size_of::<LineMeta>()
            + block.spans.len() * size_of::<Span>();
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.tick += 1;
        let tick = inner.tick;
        if let Some(old) = inner
            .map
            .insert(key.clone(), CacheEntry { block, size, tick })
        {
            inner.total -= old.size;
            inner.order.remove(&old.tick);
        }
        inner.order.insert(tick, key);
        inner.total += size;
        while inner.total > self.budget && inner.order.len() > 1 {
            let Some((_, oldest)) = inner.order.pop_first() else {
                break;
            };
            if let Some(entry) = inner.map.remove(&oldest) {
                inner.total -= entry.size;
            }
        }
    }

    pub fn forget(&self, dir: &Path) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let keys: Vec<CacheKey> = inner.map.keys().filter(|k| k.0 == dir).cloned().collect();
        for key in keys {
            if let Some(entry) = inner.map.remove(&key) {
                inner.total -= entry.size;
                inner.order.remove(&entry.tick);
            }
        }
    }
}
