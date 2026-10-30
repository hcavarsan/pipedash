use std::collections::{
    BTreeMap,
    HashMap,
};
use std::future::Future;
use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};
use std::time::Duration;

use pipedash_plugin_api::{
    AvailablePipeline,
    Organization,
    PaginatedAvailablePipelines,
    PaginatedResponse,
    PaginationParams,
    PipelineCatalog,
    Plugin as PluginTrait,
    PluginError,
};
use tokio::sync::OnceCell;
use tokio::time::Instant;

use super::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::providers::PluginAdapter;

const CATALOG_TTL: Duration = Duration::from_secs(300);
const CATALOG_MAX_ENTRIES: usize = 8;
const DEFAULT_PAGE_SIZE: usize = 100;
const SELECTION_KEY: &str = "selected_items";

enum Miss {
    Unsupported(Box<dyn PluginTrait>),
    Failed(DomainError),
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct CatalogKey {
    provider_type: String,
    config: BTreeMap<String, String>,
}

impl CatalogKey {
    fn new(provider_type: &str, config: &HashMap<String, String>) -> Self {
        Self {
            provider_type: provider_type.to_owned(),
            config: config
                .iter()
                .filter(|(key, _)| key.as_str() != SELECTION_KEY)
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }
}

struct Loaded {
    catalog: Arc<PipelineCatalog>,
    at: Instant,
}

struct Entry {
    cell: Arc<OnceCell<Loaded>>,
    seq: u64,
}

#[derive(Default)]
struct State {
    entries: HashMap<CatalogKey, Entry>,
    next_seq: u64,
}

pub(super) struct DiscoveryCache {
    ttl: Duration,
    max_entries: usize,
    state: Mutex<State>,
}

impl Default for DiscoveryCache {
    fn default() -> Self {
        Self::new(CATALOG_TTL, CATALOG_MAX_ENTRIES)
    }
}

impl DiscoveryCache {
    fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            ttl,
            max_entries,
            state: Mutex::new(State::default()),
        }
    }

    async fn get_or_fetch<E, F, Fut>(
        &self, key: CatalogKey, fetch: F,
    ) -> Result<Arc<PipelineCatalog>, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<PipelineCatalog, E>>,
    {
        let cell = self.cell(&key);
        let loaded = cell
            .get_or_try_init(|| async {
                fetch().await.map(|catalog| Loaded {
                    catalog: Arc::new(catalog),
                    at: Instant::now(),
                })
            })
            .await;

        match loaded {
            Ok(loaded) => Ok(Arc::clone(&loaded.catalog)),
            Err(e) => {
                self.discard(&key, &cell);
                Err(e)
            }
        }
    }

    fn cell(&self, key: &CatalogKey) -> Arc<OnceCell<Loaded>> {
        let now = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);

        if let Some(entry) = state.entries.get(key) {
            if !self.expired(entry, now) {
                return Arc::clone(&entry.cell);
            }
        }

        let ttl = self.ttl;
        state.entries.retain(|_, entry| {
            entry
                .cell
                .get()
                .is_none_or(|loaded| now.duration_since(loaded.at) < ttl)
        });
        while state.entries.len() >= self.max_entries {
            let Some(oldest) = state
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.seq)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            state.entries.remove(&oldest);
        }

        let cell = Arc::new(OnceCell::new());
        let seq = state.next_seq;
        state.next_seq += 1;
        state.entries.insert(
            key.clone(),
            Entry {
                cell: Arc::clone(&cell),
                seq,
            },
        );
        cell
    }

    fn expired(&self, entry: &Entry, now: Instant) -> bool {
        entry
            .cell
            .get()
            .is_some_and(|loaded| now.duration_since(loaded.at) >= self.ttl)
    }

    fn discard(&self, key: &CatalogKey, cell: &Arc<OnceCell<Loaded>>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state
            .entries
            .get(key)
            .is_some_and(|entry| Arc::ptr_eq(&entry.cell, cell) && !cell.initialized())
        {
            state.entries.remove(key);
        }
    }
}

fn matches_search(pipeline: &AvailablePipeline, needle: &str) -> bool {
    [
        Some(pipeline.name.as_str()),
        pipeline.repository.as_deref(),
        pipeline.description.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|field| field.to_lowercase().contains(needle))
}

fn serve_pipelines(
    catalog: &PipelineCatalog, org: Option<&str>, search: Option<&str>, params: &PaginationParams,
) -> PaginatedAvailablePipelines {
    let needle = search
        .map(str::trim)
        .filter(|search| !search.is_empty())
        .map(str::to_lowercase);
    let mut matched: Vec<&AvailablePipeline> = catalog
        .pipelines
        .iter()
        .filter(|pipeline| org.is_none_or(|org| pipeline.organization.as_deref() == Some(org)))
        .filter(|pipeline| {
            needle
                .as_deref()
                .is_none_or(|needle| matches_search(pipeline, needle))
        })
        .collect();
    matched.sort_by(|a, b| a.name.cmp(&b.name));

    let total = matched.len();
    let page = params.page.max(1);
    let page_size = match params.page_size {
        0 => DEFAULT_PAGE_SIZE,
        size => size,
    };
    let offset = (page - 1).saturating_mul(page_size);
    let items: Vec<AvailablePipeline> = matched
        .into_iter()
        .skip(offset)
        .take(page_size)
        .cloned()
        .collect();
    let has_more = offset.saturating_add(items.len()) < total;

    let mut response = PaginatedResponse::new(items, page, page_size, total);
    response.has_more = has_more;
    response
}

impl ProviderService {
    pub async fn discover_organizations(
        &self, provider_type: &str, config: HashMap<String, String>,
    ) -> DomainResult<Vec<Organization>> {
        match self.catalog(provider_type, config).await {
            Ok(catalog) => Ok(catalog.organizations.clone()),
            Err(Miss::Unsupported(plugin)) => plugin
                .fetch_organizations()
                .await
                .map_err(PluginAdapter::map_error),
            Err(Miss::Failed(e)) => Err(e),
        }
    }

    pub async fn discover_pipelines(
        &self, provider_type: &str, config: HashMap<String, String>, org: Option<String>,
        search: Option<String>, params: PaginationParams,
    ) -> DomainResult<PaginatedAvailablePipelines> {
        match self.catalog(provider_type, config).await {
            Ok(catalog) => Ok(serve_pipelines(
                &catalog,
                org.as_deref(),
                search.as_deref(),
                &params,
            )),
            Err(Miss::Unsupported(plugin)) => plugin
                .fetch_available_pipelines_filtered(org, search, Some(params))
                .await
                .map_err(PluginAdapter::map_error),
            Err(Miss::Failed(e)) => Err(e),
        }
    }

    async fn catalog(
        &self, provider_type: &str, config: HashMap<String, String>,
    ) -> Result<Arc<PipelineCatalog>, Miss> {
        let key = CatalogKey::new(provider_type, &config);

        self.discovery
            .get_or_fetch(key, || async {
                let plugin = self
                    .init_plugin(provider_type, config, 0)
                    .await
                    .map_err(Miss::Failed)?;

                let (validated, catalog) = tokio::join!(
                    plugin.validate_credentials(),
                    plugin.fetch_pipeline_catalog()
                );

                validated.map_err(|e| Miss::Failed(PluginAdapter::map_error(e)))?;

                match catalog {
                    Ok(catalog) => Ok(catalog),
                    Err(PluginError::NotSupported(_)) => Err(Miss::Unsupported(plugin)),
                    Err(e) => Err(Miss::Failed(PluginAdapter::map_error(e))),
                }
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use super::*;

    fn key(provider_type: &str, pairs: &[(&str, &str)]) -> CatalogKey {
        let config = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        CatalogKey::new(provider_type, &config)
    }

    fn pipeline(name: &str, org: &str, repo: &str, description: &str) -> AvailablePipeline {
        AvailablePipeline {
            id: format!("{org}/{name}"),
            name: name.to_string(),
            description: Some(description.to_string()),
            organization: Some(org.to_string()),
            repository: Some(repo.to_string()),
        }
    }

    fn catalog_of(names: &[&str]) -> PipelineCatalog {
        PipelineCatalog {
            organizations: Vec::new(),
            pipelines: names
                .iter()
                .map(|name| pipeline(name, "org", "repo", ""))
                .collect(),
        }
    }

    async fn counted(
        calls: &AtomicUsize, result: Result<PipelineCatalog, &'static str>,
    ) -> Result<PipelineCatalog, &'static str> {
        calls.fetch_add(1, Ordering::SeqCst);
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        result
    }

    #[tokio::test]
    async fn concurrent_misses_on_one_key_fetch_once() {
        let cache = DiscoveryCache::default();
        let calls = AtomicUsize::new(0);

        let (a, b) = tokio::join!(
            cache.get_or_fetch(key("prow", &[("url", "x")]), || counted(
                &calls,
                Ok(catalog_of(&["a"]))
            )),
            cache.get_or_fetch(key("prow", &[("url", "x")]), || counted(
                &calls,
                Ok(catalog_of(&["a"]))
            )),
        );

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&a.unwrap(), &b.unwrap()));
    }

    #[tokio::test]
    async fn hit_does_not_fetch_again() {
        let cache = DiscoveryCache::default();
        let calls = AtomicUsize::new(0);

        for _ in 0..3 {
            cache
                .get_or_fetch(key("prow", &[]), || counted(&calls, Ok(catalog_of(&[]))))
                .await
                .unwrap();
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_fetch_is_retried_by_the_next_call() {
        let cache = DiscoveryCache::default();
        let calls = AtomicUsize::new(0);

        let failed = cache
            .get_or_fetch(key("prow", &[]), || counted(&calls, Err("boom")))
            .await;
        assert_eq!(failed.err(), Some("boom"));

        cache
            .get_or_fetch(key("prow", &[]), || counted(&calls, Ok(catalog_of(&[]))))
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(cache.state.lock().unwrap().entries.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn expired_entry_is_fetched_again() {
        let cache = DiscoveryCache::default();
        let calls = AtomicUsize::new(0);
        let fetch = || counted(&calls, Ok(catalog_of(&[])));

        cache.get_or_fetch(key("prow", &[]), fetch).await.unwrap();
        tokio::time::advance(CATALOG_TTL - Duration::from_secs(1)).await;
        cache.get_or_fetch(key("prow", &[]), fetch).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_secs(1)).await;
        cache.get_or_fetch(key("prow", &[]), fetch).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn selected_items_does_not_split_the_key() {
        let cache = DiscoveryCache::default();
        let calls = AtomicUsize::new(0);
        let fetch = || counted(&calls, Ok(catalog_of(&[])));

        for selected in ["", "a", "a,b"] {
            cache
                .get_or_fetch(
                    key("prow", &[("url", "x"), ("selected_items", selected)]),
                    fetch,
                )
                .await
                .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        cache
            .get_or_fetch(key("prow", &[("url", "y")]), fetch)
            .await
            .unwrap();
        cache
            .get_or_fetch(key("other", &[("url", "x")]), fetch)
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn oldest_entry_is_dropped_past_the_cap() {
        let cache = DiscoveryCache::new(CATALOG_TTL, 2);
        let calls = AtomicUsize::new(0);
        let fetch = || counted(&calls, Ok(catalog_of(&[])));

        for id in ["1", "2", "3"] {
            cache
                .get_or_fetch(key("prow", &[("id", id)]), fetch)
                .await
                .unwrap();
        }
        assert_eq!(cache.state.lock().unwrap().entries.len(), 2);

        cache
            .get_or_fetch(key("prow", &[("id", "3")]), fetch)
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);

        cache
            .get_or_fetch(key("prow", &[("id", "1")]), fetch)
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }

    fn page(page: usize, page_size: usize) -> PaginationParams {
        PaginationParams { page, page_size }
    }

    fn names(response: &PaginatedAvailablePipelines) -> Vec<&str> {
        response.items.iter().map(|p| p.name.as_str()).collect()
    }

    fn sample() -> PipelineCatalog {
        PipelineCatalog {
            organizations: Vec::new(),
            pipelines: vec![
                pipeline("zeta", "kube", "kubernetes/kubernetes", "unit tests"),
                pipeline("alpha", "kube", "kubernetes/test-infra", "Lint"),
                pipeline("mid", "istio", "istio/istio", "builds the KUBE images"),
                pipeline("beta", "istio", "istio/api", "e2e"),
            ],
        }
    }

    #[test]
    fn serving_filters_by_org_and_sorts_by_name() {
        let response = serve_pipelines(&sample(), Some("kube"), None, &page(1, 10));

        assert_eq!(names(&response), ["alpha", "zeta"]);
        assert_eq!(response.total_count, 2);
        assert!(!response.has_more);
    }

    #[test]
    fn serving_search_matches_name_repository_and_description() {
        let by_name = serve_pipelines(&sample(), None, Some("  ALPH "), &page(1, 10));
        assert_eq!(names(&by_name), ["alpha"]);

        let by_repo = serve_pipelines(&sample(), None, Some("istio/api"), &page(1, 10));
        assert_eq!(names(&by_repo), ["beta"]);

        let by_description = serve_pipelines(&sample(), None, Some("kube images"), &page(1, 10));
        assert_eq!(names(&by_description), ["mid"]);

        let org_and_search = serve_pipelines(&sample(), Some("kube"), Some("KUBE"), &page(1, 10));
        assert_eq!(names(&org_and_search), ["alpha", "zeta"]);

        let none = serve_pipelines(&sample(), Some("istio"), Some("lint"), &page(1, 10));
        assert!(none.items.is_empty());
    }

    #[test]
    fn serving_blank_search_keeps_everything() {
        let response = serve_pipelines(&sample(), None, Some("   "), &page(1, 10));

        assert_eq!(response.total_count, 4);
    }

    #[test]
    fn serving_pages_and_reports_has_more() {
        let first = serve_pipelines(&sample(), None, None, &page(1, 3));
        assert_eq!(names(&first), ["alpha", "beta", "mid"]);
        assert!(first.has_more);
        assert_eq!(first.total_pages, 2);

        let last = serve_pipelines(&sample(), None, None, &page(2, 3));
        assert_eq!(names(&last), ["zeta"]);
        assert!(!last.has_more);

        let past_end = serve_pipelines(&sample(), None, None, &page(3, 3));
        assert!(past_end.items.is_empty());
        assert!(!past_end.has_more);
    }

    #[test]
    fn serving_exact_last_page_has_no_more() {
        let first = serve_pipelines(&sample(), None, None, &page(1, 2));
        assert!(first.has_more);

        let last = serve_pipelines(&sample(), None, None, &page(2, 2));
        assert_eq!(names(&last), ["mid", "zeta"]);
        assert_eq!(last.items.len(), last.page_size);
        assert!(!last.has_more);
    }

    #[test]
    fn serving_defaults_page_and_page_size() {
        let response = serve_pipelines(&sample(), None, None, &page(0, 0));

        assert_eq!(response.page, 1);
        assert_eq!(response.page_size, DEFAULT_PAGE_SIZE);
        assert_eq!(response.items.len(), 4);
    }
}
