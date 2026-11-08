use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use pipedash_plugin_api::RunHistoryOrder;
use tempfile::TempDir;

use super::{
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
    MigrationResult,
    MigrationStats,
    MigrationStep,
};
use crate::domain::{
    MetricsQuery,
    NotificationPreferences,
    OnceWatch,
    ProviderConfig,
    PushSubscription,
};
use crate::infrastructure::config::{
    PipedashConfig,
    StorageManager,
};
use crate::infrastructure::database::{
    DeviceAlertRepository,
    DevicePushRepository,
    DeviceRepository,
    MetricsRepository,
    PipelineViewRepository,
    PushRepository,
};
use crate::infrastructure::token_store::{
    MemoryTokenStore,
    TokenStore,
};
use crate::test_env::EnvGuard;

async fn create_test_manager(temp_dir: &TempDir) -> StorageManager {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = temp_dir.path().to_string_lossy().to_string();

    let token_store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new());
    StorageManager::with_token_store(config, token_store, true)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_plan_migration_no_changes() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(&temp_dir).await;

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;

    let target_config = manager.config().clone();
    let plan = orchestrator
        .plan_migration(target_config, &MigrationOptions::default())
        .unwrap();

    assert!(!plan.steps.contains(&MigrationStep::MigrateConfigs));
    assert!(!plan.steps.contains(&MigrationStep::MigrateTokens));
}

#[tokio::test]
async fn test_migration_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(&temp_dir).await;

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;

    let target_config = manager.config().clone();
    let plan = MigrationPlan {
        from: manager.config().clone(),
        to: target_config,
        steps: vec![MigrationStep::ValidateTarget],
        migrate_tokens: false,
        migrate_configs: false,
        migrate_cache: false,
        backend_changed: false,
        data_dir_changed: false,
        created_at: Utc::now(),
    };

    let result = orchestrator
        .execute_migration(
            plan,
            MigrationOptions {
                dry_run: true,
                ..Default::default()
            },
            true, // is_desktop
        )
        .await;

    assert!(result.success);
}

#[test]
fn test_migration_step_descriptions() {
    assert!(!MigrationStep::ValidateTarget.description().is_empty());
    assert!(!MigrationStep::MigrateTokens.description().is_empty());
    assert!(!MigrationStep::MigrateConfigs.description().is_empty());
}

#[test]
fn test_migration_result_summary() {
    let result = MigrationResult {
        success: true,
        steps_completed: vec![MigrationStep::MigrateConfigs],
        errors: vec![],
        duration_ms: 100,
        stats: MigrationStats {
            providers_migrated: 5,
            tokens_migrated: 3,
            cache_entries_migrated: 10,
            metrics_migrated: 0,
            permissions_migrated: 2,
            providers_cleaned: 0,
            tokens_cleaned: 0,
        },
        provider_id_mapping: std::collections::HashMap::new(),
        backups: None,
    };

    let summary = result.summary();
    assert!(summary.contains("5 providers"));
    assert!(summary.contains("10 cache entries"));
}

#[tokio::test]
async fn migrate_tokens_after_a_provider_was_deleted() {
    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    source.storage.vault_password = Some("test-password".to_string());
    let mut target = source.clone();
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::from_config(source, false).await.unwrap();
    let backend = manager.config_backend();
    let tokens = manager.token_store().await;
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        let provider = ProviderConfig {
            id: None,
            name: name.to_string(),
            provider_type: "stub".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        };
        ids.push(backend.create_provider(&provider).await.unwrap());
    }
    backend.delete_provider(ids[1]).await.unwrap();
    tokens.store_token(ids[0], "tok-a").await.unwrap();
    tokens.store_token(ids[2], "tok-c").await.unwrap();

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_tokens: true,
        token_password: Some("test-password".to_string()),
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, false).await;
    assert!(result.success, "{:?}", result.errors);

    let target_manager = StorageManager::from_config(target, false).await.unwrap();
    let mut migrated = HashMap::new();
    for provider in target_manager
        .config_backend()
        .list_providers()
        .await
        .unwrap()
    {
        let token = target_manager
            .token_store()
            .await
            .get_token(provider.id.unwrap())
            .await
            .unwrap();
        migrated.insert(provider.name, token);
    }
    assert_eq!(
        migrated,
        HashMap::from([
            ("a".to_string(), "tok-a".to_string()),
            ("c".to_string(), "tok-c".to_string()),
        ])
    );
}

#[tokio::test]
async fn migration_keeps_archived_runs_with_remapped_ids_after_restart() {
    use crate::domain::PipelineRun;
    use crate::infrastructure::database::Repository;

    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    source.storage.vault_password = Some("test-password".to_string());
    let mut target = source.clone();
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::from_config(source, false).await.unwrap();
    let backend = manager.config_backend();
    let mut ids = Vec::new();
    for name in ["deleted", "kept"] {
        ids.push(
            backend
                .create_provider(&ProviderConfig {
                    id: None,
                    name: name.to_string(),
                    provider_type: "stub".to_string(),
                    token: String::new(),
                    config: HashMap::new(),
                    refresh_interval: 30,
                    version: None,
                })
                .await
                .unwrap(),
        );
    }
    backend.delete_provider(ids[0]).await.unwrap();
    let repo = Repository::new(
        backend,
        manager.database_pool(),
        manager.token_store().await,
    );
    let source_pipeline = format!("stub__{}__HelmRelease__apps__web", ids[1]);
    let runs: Vec<PipelineRun> = (1..=105)
        .rev()
        .map(|n| {
            serde_json::from_value(serde_json::json!({
                "id": format!("release-{n}"),
                "pipeline_id": source_pipeline,
                "run_number": n,
                "status": "success",
                "started_at": "2026-10-20T00:00:00Z",
                "logs_url": "",
                "metadata": {
                    "uid": "resource-1",
                    "record_source": "status.history",
                    "release_version": n,
                    "archived": true
                }
            }))
            .unwrap()
        })
        .collect();
    repo.cache_run_history(&source_pipeline, &runs)
        .await
        .unwrap();
    let other_pipeline = format!("stub__{}__Kustomization__apps__api", ids[1]);
    let mut other = runs[0].clone();
    other.pipeline_id = other_pipeline.clone();
    repo.cache_run_history(&other_pipeline, &[other.clone()])
        .await
        .unwrap();
    let orphan_pipeline = format!("stub__{}__HelmRelease__apps__web", ids[0]);
    let mut orphan = runs[0].clone();
    orphan.id = "deleted-provider-run".to_string();
    orphan.pipeline_id = orphan_pipeline;
    repo.cache_run_history(&orphan.pipeline_id, &[orphan.clone()])
        .await
        .unwrap();

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_cache: true,
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, false).await;
    assert!(result.success, "{:?}", result.errors);
    let target_id = result.provider_id_mapping[&ids[1]];
    assert_ne!(target_id, ids[1]);
    assert_eq!(result.stats.cache_entries_migrated, 106);
    assert_eq!(
        repo.get_cached_run_history(&source_pipeline, 200, RunHistoryOrder::RunNumber)
            .await
            .unwrap(),
        runs
    );
    drop(orchestrator);
    drop(repo);
    drop(manager);

    let reopened = StorageManager::from_config(target, false).await.unwrap();
    let target_repo = Repository::new(
        reopened.config_backend(),
        reopened.database_pool(),
        reopened.token_store().await,
    );
    let target_pipeline = format!("stub__{target_id}__HelmRelease__apps__web");
    let mut expected = runs;
    for run in &mut expected {
        run.pipeline_id = target_pipeline.clone();
    }
    assert_eq!(
        target_repo
            .get_cached_run_history(&target_pipeline, 200, RunHistoryOrder::RunNumber)
            .await
            .unwrap(),
        expected
    );
    other.pipeline_id = format!("stub__{target_id}__Kustomization__apps__api");
    assert_eq!(
        target_repo
            .get_cached_run(&other.pipeline_id, other.run_number)
            .await
            .unwrap(),
        Some(other)
    );
    assert!(target_repo
        .get_cached_run_history(&source_pipeline, 200, RunHistoryOrder::RunNumber)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn migration_keeps_pipeline_metrics() {
    use crate::domain::{
        MetricEntry,
        MetricType,
    };
    use crate::infrastructure::database::RecordMetrics;

    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    source.storage.vault_password = Some("test-password".to_string());
    let mut target = source.clone();
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::from_config(source, false).await.unwrap();
    let backend = manager.config_backend();
    let mut ids = Vec::new();
    for name in ["deleted", "gh"] {
        let provider = ProviderConfig {
            id: None,
            name: name.to_string(),
            provider_type: "stub".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        };
        ids.push(backend.create_provider(&provider).await.unwrap());
    }
    backend.delete_provider(ids[0]).await.unwrap();
    let orphan_pipeline = format!("stub__{}__o__r__1", ids[0]);
    let source_pipeline = format!("stub__{}__o__r__1", ids[1]);
    let expired_pipeline = format!("stub__{}__o__r__2", ids[1]);

    let metrics = MetricsRepository::new_from_pool(manager.database_pool());
    metrics.update_global_config(true, 30).await.unwrap();
    metrics
        .upsert_pipeline_config(&source_pipeline, true, 14)
        .await
        .unwrap();
    metrics
        .upsert_pipeline_config(&orphan_pipeline, false, 3)
        .await
        .unwrap();
    let entry = |pipeline_id: &str, n: i64| MetricEntry {
        id: 0,
        pipeline_id: pipeline_id.to_string(),
        run_number: n,
        timestamp: Utc::now(),
        metric_type: MetricType::RunDuration,
        value: 60.0 * n as f64,
        metadata: None,
        created_at: Utc::now(),
        run_hash: Some(format!("hash-{n}")),
    };
    let mut entries: Vec<MetricEntry> = (1..=5).map(|n| entry(&source_pipeline, n)).collect();
    entries.push(entry(&orphan_pipeline, 1));
    metrics.insert_metrics_batch(entries).await.unwrap();
    metrics
        .update_last_processed_run(&source_pipeline, 5)
        .await
        .unwrap();
    metrics
        .update_last_processed_run(&expired_pipeline, 9)
        .await
        .unwrap();

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_tokens: true,
        migrate_cache: true,
        token_password: Some("test-password".to_string()),
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let migrate = || orchestrator.execute_migration(plan.clone(), options.clone(), false);
    let result = migrate().await;
    assert!(result.success, "{:?}", result.errors);
    assert_eq!(result.stats.metrics_migrated, 5);
    let refreshed = MetricEntry {
        run_hash: Some("hash-4b".to_string()),
        value: 999.0,
        ..entry(&source_pipeline, 4)
    };
    metrics
        .refresh_record_metrics(
            &source_pipeline,
            &[RecordMetrics {
                run_number: 4,
                run_hash: "hash-4b".to_string(),
                entries: [Some(refreshed), None, None],
            }],
        )
        .await
        .unwrap();
    let result = migrate().await;
    assert!(result.success, "{:?}", result.errors);

    let target_manager = StorageManager::from_config(target, false).await.unwrap();
    let target_id = target_manager
        .config_backend()
        .list_providers()
        .await
        .unwrap()[0]
        .id
        .unwrap();
    assert_ne!(target_id, ids[1]);
    let target_pipeline = format!("stub__{target_id}__o__r__1");
    let target_metrics = MetricsRepository::new_from_pool(target_manager.database_pool());

    let copied = target_metrics
        .query_metrics(MetricsQuery {
            pipeline_id: Some(target_pipeline.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(copied.len(), 5);
    let run_4: Vec<_> = copied
        .into_iter()
        .filter(|entry| entry.run_number == 4)
        .map(|entry| (entry.run_hash, entry.value))
        .collect();
    assert_eq!(run_4, [(Some("hash-4b".to_string()), 999.0)]);
    let config = target_metrics
        .get_pipeline_config(&target_pipeline)
        .await
        .unwrap()
        .unwrap();
    assert!(config.enabled);
    assert_eq!(config.retention_days, 14);
    let global = target_metrics.get_global_config().await.unwrap();
    assert!(global.enabled);
    assert_eq!(global.default_retention_days, 30);
    assert_eq!(
        target_metrics
            .get_last_processed_run(&target_pipeline)
            .await
            .unwrap(),
        Some(5)
    );
    assert_eq!(
        target_metrics
            .get_last_processed_run(&format!("stub__{target_id}__o__r__2"))
            .await
            .unwrap(),
        Some(9)
    );
    assert_eq!(
        target_metrics
            .export_config()
            .await
            .unwrap()
            .pipeline_configs
            .len(),
        1
    );
    assert_eq!(
        target_metrics
            .calculate_storage_stats()
            .await
            .unwrap()
            .total_metrics_count,
        5
    );
}

#[tokio::test]
async fn push_subscriptions_follow_a_storage_migration_without_the_cache() {
    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    source.storage.vault_password = Some("test-password".to_string());
    let mut target = source.clone();
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::from_config(source, false).await.unwrap();
    let push = PushRepository::new(manager.database_pool());
    let seed = |relay_id: &str, name: &str| PushSubscription {
        id: PushSubscription::local_id(relay_id),
        relay_subscription_id: relay_id.into(),
        device_name: name.into(),
        platform: "ios".into(),
        created_at: Utc::now(),
        last_success_at: None,
    };
    let paired = seed("relay-b", "iPad");
    push.import(&[seed("relay-a", "iPhone"), paired.clone()])
        .await
        .unwrap();
    push.mark_success(&paired.id).await.unwrap();
    let expected = push.list().await.unwrap();

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_tokens: false,
        migrate_cache: false,
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, false).await;
    assert!(result.success, "{:?}", result.errors);

    let target_manager = StorageManager::from_config(target, false).await.unwrap();
    let migrated = PushRepository::new(target_manager.database_pool())
        .list()
        .await
        .unwrap();
    assert_eq!(migrated, expected);
}

#[tokio::test]
async fn device_alerts_and_push_registrations_follow_a_storage_migration() {
    use crate::domain::{
        AlertKind,
        NewDeviceAlert,
    };

    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    source.storage.vault_password = Some("test-password".to_string());
    let mut target = source.clone();
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::from_config(source, false).await.unwrap();
    let backend = manager.config_backend();
    let mut ids = Vec::new();
    for name in ["deleted", "gh"] {
        let provider = ProviderConfig {
            id: None,
            name: name.to_string(),
            provider_type: "stub".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        };
        ids.push(backend.create_provider(&provider).await.unwrap());
    }
    backend.delete_provider(ids[0]).await.unwrap();
    let (gone, provider) = (ids[0], ids[1]);
    let pipeline = format!("stub__{provider}__o__r__1");
    let new_alert =
        |key: &str, kind: AlertKind, pipeline_id: Option<&str>, provider_id: i64| NewDeviceAlert {
            key: key.to_string(),
            kind,
            title: "title".to_string(),
            body: "body".to_string(),
            pipeline_id: pipeline_id.map(str::to_string),
            provider_id: Some(provider_id),
            provider_ids: vec![provider_id],
            finish: None,
        };
    let alerts = DeviceAlertRepository::new(manager.database_pool());
    for _ in 0..3 {
        alerts
            .insert(&new_alert("x", AlertKind::Outage, None, gone))
            .await
            .unwrap();
    }
    alerts.delete_all().await.unwrap();
    for new in [
        new_alert(
            &format!("pipeline:{pipeline}"),
            AlertKind::Failed,
            Some(&pipeline),
            provider,
        ),
        NewDeviceAlert {
            provider_ids: vec![provider, gone],
            ..new_alert(
                &format!("connection:{provider}"),
                AlertKind::Outage,
                None,
                provider,
            )
        },
        new_alert("connection:99", AlertKind::Outage, None, gone),
    ] {
        alerts.insert(&new).await.unwrap();
    }
    let pushes = DevicePushRepository::new(manager.database_pool());
    pushes.set("device-1", "sub-1").await.unwrap();
    pushes.mark_success("device-1").await.unwrap();
    let expected_pushes = pushes.list().await.unwrap();
    let (device, _) = crate::domain::PairedDevice::issue("Pixel", "android");
    let devices = DeviceRepository::new(manager.database_pool());
    devices.insert(&device).await.unwrap();
    devices
        .set_notification_preferences(
            &device.id,
            &NotificationPreferences {
                outage: false,
                muted_providers: vec![provider, gone],
                muted_pipelines: vec![pipeline.clone()],
                watched_pipelines: vec![pipeline.clone(), format!("stub__{gone}__x")],
                watch_once: vec![OnceWatch {
                    pipeline_id: pipeline.clone(),
                    since: chrono::DateTime::UNIX_EPOCH,
                    fired_at: Some(chrono::DateTime::UNIX_EPOCH),
                }],
                ..NotificationPreferences::default()
            },
        )
        .await
        .unwrap();

    let target_manager = StorageManager::from_config(target.clone(), false)
        .await
        .unwrap();
    DeviceAlertRepository::new(target_manager.database_pool())
        .insert(&new_alert("stale", AlertKind::Failed, None, 1))
        .await
        .unwrap();
    DevicePushRepository::new(target_manager.database_pool())
        .set("device-1", "stale-sub")
        .await
        .unwrap();
    drop(target_manager);

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_tokens: false,
        migrate_cache: false,
        clean_target: true,
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, false).await;
    assert!(result.success, "{:?}", result.errors);

    let target_manager = StorageManager::from_config(target, false).await.unwrap();
    let target_provider = target_manager
        .config_backend()
        .list_providers()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == "gh")
        .and_then(|p| p.id)
        .unwrap();
    let migrated = DeviceAlertRepository::new(target_manager.database_pool());
    let moved = migrated.list().await.unwrap();
    assert_eq!(
        moved
            .iter()
            .map(|a| (a.seq, a.key.as_str(), a.provider_id))
            .collect::<Vec<_>>(),
        vec![
            (
                4,
                format!("pipeline:stub__{target_provider}__o__r__1").as_str(),
                Some(target_provider)
            ),
            (
                5,
                format!("connection:{target_provider}").as_str(),
                Some(target_provider)
            ),
        ]
    );
    assert_eq!(
        moved[0].pipeline_id.as_deref(),
        Some(format!("stub__{target_provider}__o__r__1").as_str())
    );
    assert_eq!(moved[0].provider_ids, vec![target_provider]);
    assert_eq!(moved[1].provider_ids, vec![target_provider]);
    assert_ne!(target_provider, provider);
    assert_eq!(
        DeviceRepository::new(target_manager.database_pool())
            .notification_preferences()
            .await
            .unwrap()[&device.id],
        NotificationPreferences {
            outage: false,
            muted_providers: vec![target_provider],
            muted_pipelines: vec![format!("stub__{target_provider}__o__r__1")],
            watched_pipelines: vec![format!("stub__{target_provider}__o__r__1")],
            watch_once: vec![OnceWatch {
                pipeline_id: format!("stub__{target_provider}__o__r__1"),
                since: chrono::DateTime::UNIX_EPOCH,
                fired_at: Some(chrono::DateTime::UNIX_EPOCH),
            }],
            ..NotificationPreferences::default()
        }
    );
    assert_eq!(
        DevicePushRepository::new(target_manager.database_pool())
            .list()
            .await
            .unwrap(),
        expected_pushes
    );
    let next = migrated
        .insert(&new_alert("c", AlertKind::Failed, None, target_provider))
        .await
        .unwrap();
    assert_eq!(next.seq, 6);
}

#[tokio::test]
async fn a_storage_migration_keeps_the_target_alerts_unless_the_target_is_cleaned() {
    use crate::domain::{
        AlertKind,
        NewDeviceAlert,
    };

    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    source.storage.vault_password = Some("test-password".to_string());
    let mut target = source.clone();
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

    let new_alert = |key: &str| NewDeviceAlert {
        key: key.to_string(),
        kind: AlertKind::Failed,
        title: "title".to_string(),
        body: "body".to_string(),
        pipeline_id: None,
        provider_id: None,
        provider_ids: Vec::new(),
        finish: None,
    };
    let manager = StorageManager::from_config(source, false).await.unwrap();
    let source_alerts = DeviceAlertRepository::new(manager.database_pool());
    for key in ["source-a", "source-b"] {
        source_alerts.insert(&new_alert(key)).await.unwrap();
    }

    let target_manager = StorageManager::from_config(target.clone(), false)
        .await
        .unwrap();
    let target_alerts = DeviceAlertRepository::new(target_manager.database_pool());
    let mut target_ids = Vec::new();
    for key in ["target-a", "target-b", "target-c"] {
        target_ids.push(target_alerts.insert(&new_alert(key)).await.unwrap().id);
    }
    drop(target_manager);

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_tokens: false,
        migrate_cache: false,
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, false).await;
    assert!(result.success, "{:?}", result.errors);

    let target_manager = StorageManager::from_config(target, false).await.unwrap();
    let migrated = DeviceAlertRepository::new(target_manager.database_pool());
    let merged = migrated.list().await.unwrap();
    assert_eq!(
        merged
            .iter()
            .map(|a| (a.seq, a.key.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (1, "source-a"),
            (2, "source-b"),
            (3, "target-c"),
            (4, "target-a"),
            (5, "target-b"),
        ]
    );
    for id in &target_ids {
        assert!(migrated.get(id).await.unwrap().is_some());
    }
    assert_eq!(migrated.insert(&new_alert("next")).await.unwrap().seq, 6);
}

#[tokio::test]
async fn views_and_pins_survive_a_restart_and_a_storage_migration() {
    use crate::domain::{
        PinKind,
        SectionMode,
        ViewDisplay,
        ViewKind,
        ViewLayout,
        ViewRules,
    };
    use crate::infrastructure::database::ViewExport;

    let source_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();
    let config_path = source_dir.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

    let mut source = PipedashConfig::default();
    source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
    source.storage.vault_password = Some("test-password".to_string());
    let mut target = source.clone();
    target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

    let manager = StorageManager::from_config(source.clone(), false)
        .await
        .unwrap();
    let backend = manager.config_backend();
    let mut ids = Vec::new();
    for name in ["deleted", "gh"] {
        let provider = ProviderConfig {
            id: None,
            name: name.to_string(),
            provider_type: "stub".to_string(),
            token: String::new(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        };
        ids.push(backend.create_provider(&provider).await.unwrap());
    }
    backend.delete_provider(ids[0]).await.unwrap();
    let provider = ids[1];
    let pipeline = |n: u32| format!("stub__{provider}__o__r__{n}");

    let repo = PipelineViewRepository::new(manager.database_pool());
    let rules = ViewRules {
        provider_ids: vec![provider],
        repositories: vec!["o/*".to_string()],
        name_contains: Some("deploy".to_string()),
        statuses: Vec::new(),
    };
    let cards = ViewDisplay {
        layout: ViewLayout::Cards,
        sections: SectionMode::Provider,
        ..ViewDisplay::default()
    };
    let release = repo
        .insert_custom("Release", None, &rules, &cards)
        .await
        .unwrap();
    let prod = repo
        .insert_custom(
            "Prod",
            Some(release),
            &ViewRules::default(),
            &ViewDisplay::default(),
        )
        .await
        .unwrap();
    let eu = repo
        .insert_custom(
            "EU",
            Some(prod),
            &ViewRules::default(),
            &ViewDisplay::default(),
        )
        .await
        .unwrap();
    repo.upsert_member(eu, provider, &pipeline(1), false)
        .await
        .unwrap();
    repo.upsert_member(release, provider, &pipeline(2), true)
        .await
        .unwrap();
    let pinned = repo
        .list_views()
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.kind == ViewKind::Pinned)
        .unwrap();
    repo.update_view(
        pinned.id,
        &pinned.name,
        None,
        &pinned.rules,
        &ViewDisplay {
            layout: ViewLayout::Table,
            ..pinned.display
        },
    )
    .await
    .unwrap();
    repo.insert_pin(PinKind::View, &eu.to_string())
        .await
        .unwrap();
    repo.insert_pin(PinKind::Repository, "o/r").await.unwrap();
    repo.insert_pin(PinKind::View, &release.to_string())
        .await
        .unwrap();

    let snapshot = |export: ViewExport, provider: i64| {
        let pid = |id: &str| id.replace(&format!("__{provider}__"), "__P__");
        let view_name = |id: i64| {
            export
                .views
                .iter()
                .find(|v| v.id == id)
                .unwrap()
                .name
                .clone()
        };
        let views: Vec<_> = export
            .views
            .iter()
            .map(|v| {
                let mut rules = v.rules.clone();
                rules.provider_ids = rules
                    .provider_ids
                    .iter()
                    .map(|id| i64::from(*id == provider))
                    .collect();
                (
                    v.kind,
                    v.name.clone(),
                    v.parent_id.map(view_name),
                    v.position,
                    rules,
                    v.display,
                )
            })
            .collect();
        let members: Vec<_> = export
            .members
            .iter()
            .map(|m| {
                (
                    view_name(m.view_id),
                    pid(&m.pipeline_id),
                    m.excluded,
                    m.provider_id == provider,
                )
            })
            .collect();
        let pins: Vec<_> = export
            .pins
            .iter()
            .map(|p| match p.kind {
                PinKind::View => view_name(p.reference.parse().unwrap()),
                PinKind::Repository => p.reference.clone(),
            })
            .collect();
        format!("{views:?}\n{members:?}\n{pins:?}")
    };
    let before = snapshot(repo.export().await.unwrap(), provider);
    drop(repo);
    drop(manager);

    let manager = StorageManager::from_config(source, false).await.unwrap();
    let reopened = PipelineViewRepository::new(manager.database_pool())
        .export()
        .await
        .unwrap();
    assert_eq!(snapshot(reopened, provider), before);

    let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
    let options = MigrationOptions {
        migrate_tokens: false,
        migrate_cache: false,
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, false).await;
    assert!(result.success, "{:?}", result.errors);

    let target_manager = StorageManager::from_config(target, false).await.unwrap();
    let target_provider = target_manager
        .config_backend()
        .list_providers()
        .await
        .unwrap()[0]
        .id
        .unwrap();
    assert_ne!(target_provider, provider);
    let migrated = PipelineViewRepository::new(target_manager.database_pool())
        .export()
        .await
        .unwrap();
    assert_eq!(snapshot(migrated, target_provider), before);
}
