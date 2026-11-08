use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::RunHistoryOrder;
use serde_json::json;

use super::Repository;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    ProviderConfig,
    ResourceState,
};
use crate::infrastructure::{
    MemoryTokenStore,
    PipedashConfig,
    StorageManager,
    TokenStore,
};

async fn test_repository(dir: &tempfile::TempDir) -> (StorageManager, Repository) {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.path().display().to_string();
    let manager = StorageManager::with_token_store(config, Arc::new(MemoryTokenStore::new()), true)
        .await
        .unwrap();
    let repository = Repository::new(
        manager.config_backend(),
        manager.database_pool(),
        manager.token_store().await,
    );

    (manager, repository)
}

fn cached_run(pipeline_id: &str, run_number: i64, started_at: &str) -> PipelineRun {
    serde_json::from_value(json!({
        "id": format!("{pipeline_id}__{run_number}"), "pipeline_id": pipeline_id,
        "run_number": run_number, "status": "success", "started_at": started_at,
        "logs_url": "", "metadata": {}
    }))
    .unwrap()
}

async fn run_pages(
    repository: &Repository, pipeline_id: &str, order: RunHistoryOrder,
) -> Vec<Vec<i64>> {
    let mut pages = Vec::new();
    for page in 1..=3 {
        let runs = repository
            .get_paginated_runs(pipeline_id, page, 2, order)
            .await
            .unwrap();
        pages.push(runs.iter().map(|run| run.run_number).collect());
    }

    pages
}

#[tokio::test]
async fn pipeline_cache_keeps_resource_state_and_stores_state_only_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (manager, repository) = test_repository(&dir).await;
    let provider_id = manager
        .config_backend()
        .create_provider(&ProviderConfig {
            id: None,
            name: "Stub".to_string(),
            provider_type: "stub".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        })
        .await
        .unwrap();
    let mut pipeline: Pipeline = serde_json::from_value(json!({
        "id": "stub__1__Kustomization__apps__web", "provider_id": provider_id,
        "provider_type": "stub", "name": "Kustomization/web", "status": "running",
        "repository": "apps/source", "last_updated": "2026-10-21T00:00:00Z",
        "resource_state": { "label": "Reconciling", "message": "Applying revision a" },
        "metadata": { "uid": "resource-1" }
    }))
    .unwrap();
    let mut stateless = pipeline.clone();
    stateless.id = "stub__1__Kustomization__apps__api".to_string();
    stateless.resource_state = None;

    repository
        .update_pipelines_cache(provider_id, &[pipeline.clone(), stateless])
        .await
        .unwrap();
    let first = repository
        .get_cached_pipelines(Some(provider_id))
        .await
        .unwrap();
    let stored = |pipelines: &[Pipeline], id: &str| {
        pipelines
            .iter()
            .find(|candidate| candidate.id == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(
        stored(&first, &pipeline.id).resource_state,
        pipeline.resource_state
    );
    assert_eq!(stored(&first, &pipeline.id).metadata, pipeline.metadata);
    assert_eq!(
        stored(&first, "stub__1__Kustomization__apps__api").resource_state,
        None
    );

    pipeline.resource_state = Some(ResourceState::new(
        "Reconciling",
        Some("Applying revision b"),
    ));
    repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    let changed = repository.get_cached_pipelines(None).await.unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].resource_state, pipeline.resource_state);
    assert_eq!(changed[0].status, pipeline.status);
}

fn stub_provider() -> ProviderConfig {
    ProviderConfig {
        id: None,
        name: "ci-stub".to_string(),
        provider_type: "stub".to_string(),
        token: String::new(),
        config: HashMap::new(),
        refresh_interval: 60,
        version: None,
    }
}

#[tokio::test]
async fn get_provider_reads_a_missing_token_entry_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (manager, repository) = test_repository(&dir).await;
    let id = manager
        .config_backend()
        .create_provider(&stub_provider())
        .await
        .unwrap();

    let provider = repository.get_provider(id).await.unwrap();

    assert_eq!(provider.name, "ci-stub");
    assert_eq!(provider.token, "");
}

struct FailingTokenStore;

#[async_trait::async_trait]
impl TokenStore for FailingTokenStore {
    async fn store_token(&self, _provider_id: i64, _token: &str) -> DomainResult<()> {
        Ok(())
    }

    async fn get_token(&self, _provider_id: i64) -> DomainResult<String> {
        Err(DomainError::AuthenticationFailed(
            "vault is locked".to_string(),
        ))
    }

    async fn delete_token(&self, _provider_id: i64) -> DomainResult<()> {
        Ok(())
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        Ok(HashMap::new())
    }
}

#[tokio::test]
async fn get_provider_fails_when_the_token_store_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (manager, _) = test_repository(&dir).await;
    let id = manager
        .config_backend()
        .create_provider(&stub_provider())
        .await
        .unwrap();
    let repository = Repository::new(
        manager.config_backend(),
        manager.database_pool(),
        Arc::new(FailingTokenStore),
    );

    let result = repository.get_provider(id).await;

    assert!(matches!(result, Err(DomainError::AuthenticationFailed(_))));
}

#[tokio::test]
async fn run_cache_orders_by_the_requested_history_order_on_every_page() {
    let dir = tempfile::tempdir().unwrap();
    let (_manager, repository) = test_repository(&dir).await;
    let bounded = "stub-bounded__1__Kustomization__apps__web";
    let other = "stub__1__acme__web";
    let runs = |pipeline_id: &str| {
        [
            cached_run(pipeline_id, 100, "2026-10-22T10:00:00Z"),
            cached_run(pipeline_id, 200, "2026-10-20T10:00:00Z"),
            cached_run(pipeline_id, 300, "2026-10-21T10:00:00.500Z"),
            cached_run(pipeline_id, 400, "2026-10-21T10:00:00Z"),
            cached_run(pipeline_id, 500, "2026-10-21T10:00:00Z"),
        ]
    };
    repository
        .cache_run_history(bounded, &runs(bounded))
        .await
        .unwrap();
    repository
        .cache_run_history(other, &runs(other))
        .await
        .unwrap();

    assert_eq!(
        run_pages(&repository, bounded, RunHistoryOrder::StartedAt).await,
        vec![vec![100, 300], vec![500, 400], vec![200]]
    );
    assert_eq!(
        run_pages(&repository, other, RunHistoryOrder::RunNumber).await,
        vec![vec![500, 400], vec![300, 200], vec![100]]
    );
    assert_eq!(
        run_pages(&repository, other, RunHistoryOrder::StartedAt).await,
        vec![vec![100, 300], vec![500, 400], vec![200]]
    );
    assert_eq!(
        run_pages(&repository, bounded, RunHistoryOrder::RunNumber).await,
        vec![vec![500, 400], vec![300, 200], vec![100]]
    );

    let mut reconciled_again = runs(bounded)[1].clone();
    reconciled_again.started_at = "2026-10-23T10:00:00Z".parse().unwrap();
    let mut archived = runs(bounded)[0].clone();
    archived
        .metadata
        .insert("archived".to_string(), json!(true));
    repository
        .merge_run_cache(
            bounded,
            Vec::new(),
            vec![reconciled_again, archived],
            Vec::new(),
        )
        .await
        .unwrap();

    assert_eq!(
        run_pages(&repository, bounded, RunHistoryOrder::StartedAt).await,
        vec![vec![200, 100], vec![300, 500], vec![400]]
    );
    let newest = repository
        .get_cached_run_history(bounded, 3, RunHistoryOrder::StartedAt)
        .await
        .unwrap();
    assert_eq!(
        newest.iter().map(|run| run.run_number).collect::<Vec<_>>(),
        vec![200, 100, 300]
    );
    let kept = repository
        .get_cached_run(bounded, 100)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.metadata["archived"], json!(true));
    assert_eq!(repository.get_cached_run_count(bounded).await.unwrap(), 5);
}
