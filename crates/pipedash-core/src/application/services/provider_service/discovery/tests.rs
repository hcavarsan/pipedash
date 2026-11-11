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
        cache.get_or_fetch(key("stub", &[("url", "x")]), || counted(
            &calls,
            Ok(catalog_of(&["a"]))
        )),
        cache.get_or_fetch(key("stub", &[("url", "x")]), || counted(
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
            .get_or_fetch(key("stub", &[]), || counted(&calls, Ok(catalog_of(&[]))))
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
        .get_or_fetch(key("stub", &[]), || counted(&calls, Err("boom")))
        .await;
    assert_eq!(failed.err(), Some("boom"));

    cache
        .get_or_fetch(key("stub", &[]), || counted(&calls, Ok(catalog_of(&[]))))
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

    cache.get_or_fetch(key("stub", &[]), fetch).await.unwrap();
    tokio::time::advance(CATALOG_TTL - Duration::from_secs(1)).await;
    cache.get_or_fetch(key("stub", &[]), fetch).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    tokio::time::advance(Duration::from_secs(1)).await;
    cache.get_or_fetch(key("stub", &[]), fetch).await.unwrap();
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
                key("stub", &[("url", "x"), ("selected_items", selected)]),
                fetch,
            )
            .await
            .unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    cache
        .get_or_fetch(key("stub", &[("url", "y")]), fetch)
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
            .get_or_fetch(key("stub", &[("id", id)]), fetch)
            .await
            .unwrap();
    }
    assert_eq!(cache.state.lock().unwrap().entries.len(), 2);

    cache
        .get_or_fetch(key("stub", &[("id", "3")]), fetch)
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);

    cache
        .get_or_fetch(key("stub", &[("id", "1")]), fetch)
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

#[tokio::test]
async fn previews_normalize_the_requested_page() {
    let h = crate::application::test_support::Harness::new().await;
    let preview = |page, size| {
        let service = Arc::clone(&h.provider_service);
        async move {
            service
                .preview_pipelines("stub", HashMap::new(), None, None, page, size)
                .await
                .unwrap()
        }
    };

    let defaults = preview(None, None).await;
    assert_eq!((defaults.page, defaults.page_size), (1, DEFAULT_PAGE_SIZE));

    let clamped_low = preview(Some(0), Some(1)).await;
    assert_eq!(
        (clamped_low.page, clamped_low.page_size),
        (1, MIN_PAGE_SIZE)
    );

    let clamped_high = preview(Some(3), Some(10_000)).await;
    assert_eq!(
        (clamped_high.page, clamped_high.page_size),
        (3, MAX_PAGE_SIZE)
    );
}

fn organization(id: &str) -> Organization {
    Organization {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
    }
}

async fn harness_with_catalog(
    catalog: PipelineCatalog,
) -> crate::application::test_support::Harness {
    let h = crate::application::test_support::Harness::new().await;
    h.provider_service
        .discovery
        .get_or_fetch(key("stub", &[]), || async { Ok::<_, ()>(catalog) })
        .await
        .unwrap();
    h
}

#[tokio::test]
async fn matching_ids_cover_every_match_beyond_one_page() {
    let mut catalog = sample();
    catalog
        .pipelines
        .extend((0..250).map(|n| pipeline(&format!("bulk-{n:03}"), "kube", "kubernetes/bulk", "")));
    let h = harness_with_catalog(catalog).await;
    let matching = |org: Option<&str>, search: Option<&str>| {
        h.provider_service.matching_pipeline_ids(
            "stub",
            HashMap::new(),
            org.map(str::to_string),
            search.map(str::to_string),
        )
    };

    assert_eq!(matching(Some("kube"), None).await.unwrap().len(), 252);
    assert_eq!(matching(None, None).await.unwrap().len(), 254);
    assert_eq!(
        matching(Some("kube"), Some("ALPHA")).await.unwrap(),
        ["kube/alpha"]
    );
    assert!(matching(Some("istio"), Some("alpha"))
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn matching_ids_use_the_only_organization_when_none_is_given() {
    let mut catalog = sample();
    catalog.organizations = vec![organization("kube")];
    let h = harness_with_catalog(catalog).await;

    let ids = h
        .provider_service
        .matching_pipeline_ids("stub", HashMap::new(), Some(String::new()), None)
        .await
        .unwrap();

    assert_eq!(ids, ["kube/alpha", "kube/zeta"]);

    let mut two = sample();
    two.organizations = vec![organization("kube"), organization("istio")];
    let h = harness_with_catalog(two).await;
    let all = h
        .provider_service
        .matching_pipeline_ids("stub", HashMap::new(), None, None)
        .await
        .unwrap();

    assert_eq!(all.len(), 4);
}

#[tokio::test]
async fn matching_ids_page_through_plugins_without_a_catalog() {
    let h = crate::application::test_support::Harness::new().await;

    let ids = h
        .provider_service
        .matching_pipeline_ids("stub", HashMap::new(), None, None)
        .await
        .unwrap();

    assert!(ids.is_empty());
}
