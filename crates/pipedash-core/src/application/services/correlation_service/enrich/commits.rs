use std::collections::{
    BTreeMap,
    HashMap,
};

use pipedash_plugin_api::{
    ChangedFile,
    CommitComparison,
    CommitInfo,
    CommitSha,
    RepoId,
};

use super::super::CorrelationService;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::{
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::database::ChangeRow;

pub(super) const CAPACITY: usize = 2048;
const AUTHORED_BATCH: i64 = 50;

#[derive(Clone)]
pub(super) enum Cached {
    Commit(CommitInfo),
    Comparison(CommitComparison),
    Changes(Vec<ChangedFile>),
    Seen,
}

pub(super) struct Lru {
    capacity: usize,
    tick: u64,
    entries: HashMap<String, (u64, Cached)>,
    order: BTreeMap<u64, String>,
}

impl Lru {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            tick: 0,
            entries: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    pub(super) fn get(&mut self, key: &str) -> Option<Cached> {
        let (tick, value) = self.entries.get_mut(key)?;
        self.order.remove(tick);
        self.tick += 1;
        *tick = self.tick;
        self.order.insert(self.tick, key.to_string());
        Some(value.clone())
    }

    pub(super) fn put(&mut self, key: String, value: Cached) {
        self.tick += 1;
        if let Some((old, _)) = self.entries.insert(key.clone(), (self.tick, value)) {
            self.order.remove(&old);
        }
        self.order.insert(self.tick, key);
        while self.entries.len() > self.capacity {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
}

impl CorrelationService {
    pub(super) async fn provider_configs(&self) -> Vec<ProviderConfig> {
        let mut configs = match self.repository.list_providers().await {
            Ok(configs) => configs,
            Err(error) => {
                tracing::debug!(%error, "Provider list unavailable for enrichment");
                return Vec::new();
            }
        };
        configs.retain(|config| config.id.is_some());
        configs.sort_by_key(|config| config.id);
        configs
    }

    pub(in crate::application::services::correlation_service) async fn route(
        &self, repo: &RepoId,
    ) -> Option<ProviderLease> {
        for config in self.provider_configs().await {
            let Some(id) = config.id else {
                continue;
            };
            let Ok(lease) = self.provider_service.acquire(id).await else {
                continue;
            };
            let hosts = lease.provider.source_hosts(&config.config);
            if hosts
                .iter()
                .any(|host| host.eq_ignore_ascii_case(repo.host()))
            {
                return Some(lease);
            }
        }
        None
    }

    async fn routed(&self, repo: &RepoId) -> Option<ProviderLease> {
        let lease = self.route(repo).await?;
        if self.provider_service.check_quota(&lease).await.is_some() {
            return None;
        }
        Some(lease)
    }

    pub(in crate::application::services::correlation_service) async fn commit_info(
        &self, repo: &RepoId, rev: &str,
    ) -> Option<CommitInfo> {
        let key = format!("commit|{}|{rev}", repo.as_str());
        if let Some(Cached::Commit(info)) = self.enrich.recall(&key) {
            return Some(info);
        }
        let lease = self.routed(repo).await?;
        let info = quiet(lease.provider.resolve_commit(repo, rev).await)?;
        self.enrich.remember(key, Cached::Commit(info.clone()));
        Some(info)
    }

    pub(in crate::application::services::correlation_service) async fn compare(
        &self, repo: &RepoId, base: &CommitSha, head: &CommitSha,
    ) -> Option<CommitComparison> {
        let key = format!(
            "compare|{}|{}|{}",
            repo.as_str(),
            base.as_str(),
            head.as_str()
        );
        if let Some(Cached::Comparison(comparison)) = self.enrich.recall(&key) {
            return Some(comparison);
        }
        let lease = self.routed(repo).await?;
        let comparison = quiet(lease.provider.compare_commits(repo, base, head).await)?;
        self.enrich
            .remember(key, Cached::Comparison(comparison.clone()));
        Some(comparison)
    }

    pub(in crate::application::services::correlation_service) async fn changes_of(
        &self, repo: &RepoId, sha: &CommitSha,
    ) -> Option<Vec<ChangedFile>> {
        let key = format!("changes|{}|{}", repo.as_str(), sha.as_str());
        if let Some(Cached::Changes(files)) = self.enrich.recall(&key) {
            return Some(files);
        }
        let lease = self.routed(repo).await?;
        let files = quiet(lease.provider.commit_changes(repo, sha).await)?;
        self.enrich.remember(key, Cached::Changes(files.clone()));
        Some(files)
    }

    pub(super) async fn fill_authored_at(&self) -> DomainResult<()> {
        let pending = self
            .store
            .conn()
            .list_changes_without_authored_at(AUTHORED_BATCH)
            .await?;
        for change in pending {
            let (Some(repo), Some(sha)) = (
                stored_repo(&change.repo),
                CommitSha::parse(&change.commit_sha),
            ) else {
                continue;
            };
            let Some(info) = self.commit_info(&repo, sha.as_str()).await else {
                continue;
            };
            let row = ChangeRow {
                authored_at: Some(info.authored_at),
                ..change
            };
            self.store.conn().upsert_change(&row).await?;
        }
        Ok(())
    }
}

pub(super) fn stored_repo(stored: &str) -> Option<RepoId> {
    let (host, path) = stored.split_once('/')?;
    RepoId::from_host_path(host, path)
}

fn quiet<T>(result: DomainResult<T>) -> Option<T> {
    result
        .inspect_err(|error| tracing::debug!(%error, "Commit lookup failed"))
        .ok()
}

#[cfg(test)]
mod tests;
