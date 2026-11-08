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
const MIN_PAGE_SIZE: usize = 10;
const MAX_PAGE_SIZE: usize = 200;
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

    pub async fn preview_pipelines(
        &self, provider_type: &str, config: HashMap<String, String>, org: Option<String>,
        search: Option<String>, page: Option<usize>, page_size: Option<usize>,
    ) -> DomainResult<PaginatedAvailablePipelines> {
        let params = PaginationParams {
            page: page.unwrap_or(1).max(1),
            page_size: page_size
                .unwrap_or(DEFAULT_PAGE_SIZE)
                .clamp(MIN_PAGE_SIZE, MAX_PAGE_SIZE),
        };

        self.discover_pipelines(provider_type, config, org, search, params)
            .await
    }

    pub async fn matching_pipeline_ids(
        &self, provider_type: &str, config: HashMap<String, String>, organization: Option<String>,
        search: Option<String>,
    ) -> DomainResult<Vec<String>> {
        let organization = organization.filter(|organization| !organization.is_empty());
        match self.catalog(provider_type, config).await {
            Ok(catalog) => {
                let organization =
                    organization.or_else(|| sole_organization(&catalog.organizations));
                let params = PaginationParams {
                    page: 1,
                    page_size: catalog.pipelines.len().max(1),
                };
                let matched = serve_pipelines(
                    &catalog,
                    organization.as_deref(),
                    search.as_deref(),
                    &params,
                );
                Ok(matched
                    .items
                    .into_iter()
                    .map(|pipeline| pipeline.id)
                    .collect())
            }
            Err(Miss::Unsupported(plugin)) => {
                paged_matching_ids(plugin.as_ref(), organization, search).await
            }
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

const MATCHING_PAGE_SIZE: usize = 200;
const MATCHING_MAX_PAGES: usize = 500;

fn sole_organization(organizations: &[Organization]) -> Option<String> {
    match organizations {
        [only] => Some(only.id.clone()),
        _ => None,
    }
}

async fn paged_matching_ids(
    plugin: &dyn PluginTrait, organization: Option<String>, search: Option<String>,
) -> DomainResult<Vec<String>> {
    let organization = match organization {
        Some(organization) => Some(organization),
        None => sole_organization(
            &plugin
                .fetch_organizations()
                .await
                .map_err(PluginAdapter::map_error)?,
        ),
    };
    let mut ids = Vec::new();
    for page in 1..=MATCHING_MAX_PAGES {
        let params = PaginationParams {
            page,
            page_size: MATCHING_PAGE_SIZE,
        };
        let response = plugin
            .fetch_available_pipelines_filtered(organization.clone(), search.clone(), Some(params))
            .await
            .map_err(PluginAdapter::map_error)?;
        let has_more = response.has_more;
        ids.extend(response.items.into_iter().map(|pipeline| pipeline.id));
        if !has_more {
            break;
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests;
