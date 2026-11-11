use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;
use std::time::SystemTime;

use super::registry::{
    lock,
    remove_doc,
    remove_run,
    Registry,
};
use super::{
    io_error,
    join_error,
    DocKey,
    LogStore,
    RunId,
    TrimScope,
};
use crate::domain::{
    DomainResult,
    LogCacheStats,
};
use crate::infrastructure::run_logs::CACHE_MAX_IDLE;

impl LogStore {
    pub(super) fn evict(
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
