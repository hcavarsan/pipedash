use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use pipedash_plugin_api::RunHistoryOrder;
use tempfile::TempDir;

use super::*;
use crate::application::test_support::plugins::registry;
use crate::domain::{
    MetricEntry,
    MetricType,
    MetricsQuery,
    PipelineRun,
    ProviderConfig,
};
use crate::infrastructure::config::analysis::analyze_config;
use crate::infrastructure::config::PipedashConfig;
use crate::infrastructure::database::{
    MetricsRepository,
    Repository,
};
use crate::infrastructure::token_store::MemoryTokenStore;
use crate::test_env::EnvGuard;

async fn migration_uses_confirmed_data_selection(migrate_data: bool) {
    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    let _env = EnvGuard::set(&[
        ("PIPEDASH_CONFIG_PATH", config_path.to_str()),
        ("PIPEDASH_VAULT_PASSWORD", None),
    ]);
    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    std::fs::write(&config_path, toml::to_string(&source).unwrap()).unwrap();
    let source_tokens = Arc::new(MemoryTokenStore::new());
    let manager = StorageManager::with_token_store(source.clone(), source_tokens, true)
        .await
        .unwrap();
    let provider_id = manager
        .config_backend()
        .create_provider(&ProviderConfig {
            id: None,
            name: "saved-provider".to_string(),
            provider_type: "stub".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        })
        .await
        .unwrap();
    let pipeline_id = format!("stub__{provider_id}__pipeline");
    let run: PipelineRun = serde_json::from_value(serde_json::json!({
        "id": "saved-run",
        "pipeline_id": pipeline_id,
        "run_number": 7,
        "status": "success",
        "started_at": "2026-10-20T00:00:00Z",
        "logs_url": ""
    }))
    .unwrap();
    let source_repo = Repository::new(
        manager.config_backend(),
        manager.database_pool(),
        manager.token_store().await,
    );
    source_repo
        .cache_run_history(&pipeline_id, std::slice::from_ref(&run))
        .await
        .unwrap();
    let metrics = MetricsRepository::new_from_pool(manager.database_pool());
    metrics
        .insert_metrics_batch(vec![MetricEntry {
            id: 0,
            pipeline_id: pipeline_id.clone(),
            run_number: 7,
            timestamp: Utc::now(),
            metric_type: MetricType::RunDuration,
            value: 42.0,
            metadata: None,
            created_at: Utc::now(),
            run_hash: Some("saved-hash".to_string()),
        }])
        .await
        .unwrap();

    let mut target = source;
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();
    let target_tokens = Arc::new(MemoryTokenStore::new());
    let orchestrator =
        MigrationOrchestrator::from_manager(&manager, None, Some(target_tokens.clone())).await;
    let analysis = analyze_config(
        &toml::to_string(&target).unwrap(),
        &manager,
        None,
        Some(target_tokens.clone()),
        &registry(),
    )
    .await
    .unwrap();
    assert!(analysis.valid, "{:?}", analysis.errors);
    let mut plan = analysis.migration_plan.unwrap();
    if !migrate_data {
        plan = orchestrator
            .plan_migration(
                target.clone(),
                &MigrationOptions {
                    migrate_cache: true,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let result = orchestrator
        .execute_migration(
            plan,
            MigrationOptions {
                migrate_cache: migrate_data,
                ..Default::default()
            },
            true,
        )
        .await;
    assert!(result.success, "{:?}", result.errors);

    let reopened = StorageManager::with_token_store(target, target_tokens, true)
        .await
        .unwrap();
    let target_id = result.provider_id_mapping[&provider_id];
    let target_pipeline = format!("stub__{target_id}__pipeline");
    let target_repo = Repository::new(
        reopened.config_backend(),
        reopened.database_pool(),
        reopened.token_store().await,
    );
    let copied_runs = target_repo
        .get_cached_run_history(&target_pipeline, 10, RunHistoryOrder::RunNumber)
        .await
        .unwrap();
    let copied_metrics = MetricsRepository::new_from_pool(reopened.database_pool())
        .query_metrics(MetricsQuery {
            pipeline_id: Some(target_pipeline.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    if migrate_data {
        let mut expected = run.clone();
        expected.pipeline_id = target_pipeline;
        assert_eq!(copied_runs, [expected]);
        assert_eq!(
            copied_metrics
                .iter()
                .map(|entry| (entry.run_number, entry.value, entry.run_hash.as_deref()))
                .collect::<Vec<_>>(),
            [(7, 42.0, Some("saved-hash"))]
        );
    } else {
        assert!(copied_runs.is_empty());
        assert!(copied_metrics.is_empty());
    }
    assert_eq!(
        source_repo
            .get_cached_run_history(&pipeline_id, 10, RunHistoryOrder::RunNumber)
            .await
            .unwrap(),
        [run]
    );
    assert_eq!(
        metrics
            .query_metrics(MetricsQuery::default())
            .await
            .unwrap()[0]
            .value,
        42.0
    );
}

#[tokio::test]
async fn editor_migration_copies_data_selected_after_analysis() {
    migration_uses_confirmed_data_selection(true).await;
}

#[tokio::test]
async fn migration_skips_data_deselected_after_planning() {
    migration_uses_confirmed_data_selection(false).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn postgres_connection_change_copies_providers_and_history_after_restart() {
    use sqlx::Connection;

    let root = TempDir::new().unwrap();
    let config_path = root.path().join("config.toml");
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let mut admin = sqlx::PgConnection::connect(&database_url).await.unwrap();
    let suffix: String = root
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let mut urls = Vec::new();
    for side in ["source", "target"] {
        let database = format!("migration_{suffix}_{side}").to_ascii_lowercase();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&mut admin)
            .await
            .unwrap();
        let mut url = url::Url::parse(&database_url).unwrap();
        url.set_path(&database);
        urls.push(url.to_string());
    }
    let mut source = PipedashConfig::default();
    source.storage.data_dir = root.path().to_string_lossy().to_string();
    source.storage.backend = crate::infrastructure::config::StorageBackend::Postgres;
    source.storage.postgres.connection_string = urls[0].clone();
    source.storage.vault_password = Some("test-password".to_string());
    std::fs::write(&config_path, toml::to_string(&source).unwrap()).unwrap();
    let mut target = source.clone();
    target.storage.postgres.connection_string = urls[1].clone();
    let manager = StorageManager::from_config(source, false).await.unwrap();
    let provider_id = manager
        .config_backend()
        .create_provider(&ProviderConfig {
            id: None,
            name: "saved-provider".to_string(),
            provider_type: "stub".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        })
        .await
        .unwrap();
    manager
        .token_store()
        .await
        .store_token(provider_id, "saved-token")
        .await
        .unwrap();
    let pipeline_id = format!("stub__{provider_id}__pipeline");
    let run: PipelineRun = serde_json::from_value(serde_json::json!({
        "id": "saved-run",
        "pipeline_id": pipeline_id,
        "run_number": 7,
        "status": "success",
        "started_at": "2026-10-20T00:00:00Z",
        "logs_url": ""
    }))
    .unwrap();
    let source_repo = Repository::new(
        manager.config_backend(),
        manager.database_pool(),
        manager.token_store().await,
    );
    source_repo
        .cache_run_history(&pipeline_id, std::slice::from_ref(&run))
        .await
        .unwrap();
    let analysis = analyze_config(
        &toml::to_string(&target).unwrap(),
        &manager,
        None,
        None,
        &registry(),
    )
    .await
    .unwrap();
    assert!(analysis.valid, "{:?}", analysis.errors);
    assert!(analysis.postgres_connection.unwrap().success);
    let analyzed_plan = analysis
        .migration_plan
        .expect("connection change needs migration");
    assert!(analyzed_plan.migrate_configs);
    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_cache: true,
        migrate_tokens: true,
        token_password: Some("test-password".to_string()),
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, false).await;
    assert!(result.success, "{:?}", result.errors);
    let reopened = StorageManager::from_config(target, false).await.unwrap();
    let providers = reopened.config_backend().list_providers().await.unwrap();
    assert_eq!(
        providers
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["saved-provider"]
    );
    let target_id = providers[0].id.unwrap();
    assert_eq!(
        reopened
            .token_store()
            .await
            .get_token(target_id)
            .await
            .unwrap(),
        "saved-token"
    );
    let target_pipeline = format!("stub__{target_id}__pipeline");
    let target_repo = Repository::new(
        reopened.config_backend(),
        reopened.database_pool(),
        reopened.token_store().await,
    );
    let mut expected = run.clone();
    expected.pipeline_id = target_pipeline.clone();
    assert_eq!(
        target_repo
            .get_cached_run_history(&target_pipeline, 10, RunHistoryOrder::RunNumber)
            .await
            .unwrap(),
        [expected]
    );
    assert_eq!(
        source_repo
            .get_cached_run_history(&pipeline_id, 10, RunHistoryOrder::RunNumber)
            .await
            .unwrap(),
        [run]
    );
    drop(source_repo);
    drop(target_repo);
    drop(orchestrator);
    drop(manager);
    drop(reopened);
    for connection_string in urls {
        let url = url::Url::parse(&connection_string).unwrap();
        let database = url.path().trim_start_matches('/');
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {database} WITH (FORCE)"
        )))
        .execute(&mut admin)
        .await
        .unwrap();
    }
}
