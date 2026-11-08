use std::collections::HashMap;

use super::*;
use crate::application::test_support::{
    FailOnceTokenStore,
    RecordingBus,
};
use crate::context::http_client_manager;
use crate::infrastructure::database::{
    init_database,
    Repository,
};
use crate::infrastructure::{
    ConfigLoader,
    ConfigState,
    PipedashConfig,
    ProviderFileConfig,
    SqliteConfigBackend,
};

pub(crate) async fn context(dir: &std::path::Path, tokens: Arc<dyn TokenStore>) -> CoreContext {
    context_with_bus(dir, tokens, Arc::new(NoOpEventBus)).await
}

async fn context_with_bus(
    dir: &std::path::Path, tokens: Arc<dyn TokenStore>, bus: Arc<dyn EventBus>,
) -> CoreContext {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.display().to_string();
    config.general.metrics_enabled = false;
    config.general.default_refresh_interval = 73;
    let path = dir.join("config.toml");
    std::fs::write(&path, ConfigLoader::to_toml(&config).unwrap()).unwrap();
    let plugin_registry = crate::application::test_support::plugins::registry();
    let config_state = ConfigState::from_config(
        config,
        path,
        Arc::clone(&tokens),
        Arc::clone(&plugin_registry),
    );
    let pool = init_database(dir.join("pipedash.db")).await.unwrap();
    let repository = Arc::new(Repository::new(
        Arc::new(SqliteConfigBackend::new(pool.clone())),
        infrastructure::database::DatabasePool::Sqlite(pool),
        Arc::clone(&tokens),
    ));
    let http = http_client_manager("").unwrap();
    let log_store = infrastructure::run_logs::LogStore::open(dir.join("run-logs"), u64::MAX)
        .await
        .unwrap();
    let services = application::Services::new(
        repository,
        Arc::clone(&http),
        None,
        Arc::clone(&bus),
        Arc::clone(&config_state),
        log_store,
        plugin_registry,
    );
    CoreContext::from_services(
        bus,
        tokens,
        http,
        None,
        config_state,
        services,
        Arc::default(),
    )
}

async fn define_provider(core: &CoreContext, key: &str) {
    let path = core.config_state.config_path().clone();
    let mut config = ConfigLoader::load(&path).unwrap();
    config.providers.insert(
        key.into(),
        ProviderFileConfig {
            name: None,
            provider_type: "stub".into(),
            token: "storage:1".into(),
            refresh_interval: 30,
            config: HashMap::new(),
        },
    );
    std::fs::write(&path, ConfigLoader::to_toml(&config).unwrap()).unwrap();
    core.config_state.reload().await.unwrap();
}

fn provider(key: &str) -> ProviderConfig {
    ProviderConfig {
        id: None,
        name: key.into(),
        provider_type: "stub".into(),
        token: "saved-token".into(),
        config: HashMap::new(),
        refresh_interval: 30,
        version: None,
    }
}

#[tokio::test]
async fn reset_removes_config_only_providers_and_persisted_metrics_when_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = Arc::new(FailOnceTokenStore::default());
    let core = context(dir.path(), tokens.clone()).await;
    define_provider(&core, "ci-a").await;
    define_provider(&core, "config-only").await;
    core.provider_service
        .repository()
        .add_provider(&provider("ci-a"))
        .await
        .unwrap();
    tokens.store_token(999, "orphan-token").await.unwrap();
    let pool = core
        .provider_service
        .repository()
        .cache_pool()
        .as_sqlite()
        .unwrap();
    sqlx::query("INSERT INTO pipeline_metrics (pipeline_id, timestamp, metric_type, value) VALUES ('retained', '2026-10-26T00:00:00Z', 'duration', 1)").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO metrics_processing_state (pipeline_id) VALUES ('retained')")
        .execute(pool)
        .await
        .unwrap();
    std::fs::write(dir.path().join("export.toml"), "user export").unwrap();

    assert_eq!(core.factory_reset().await.unwrap(), 1);

    assert!(core
        .provider_service
        .list_providers()
        .await
        .unwrap()
        .is_empty());
    assert!(tokens.get_all_tokens().await.unwrap().is_empty());
    core.config_state.reload().await.unwrap();
    assert!(core.config_state.get().await.providers.is_empty());
    assert_eq!(
        core.config_state
            .get()
            .await
            .general
            .default_refresh_interval,
        73
    );
    core.config_state
        .sync_providers_from_toml(
            core.provider_service.repository().config_backend().as_ref(),
            false,
        )
        .await
        .unwrap();
    assert!(core
        .provider_service
        .list_providers()
        .await
        .unwrap()
        .is_empty());
    let metrics: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_metrics")
        .fetch_one(pool)
        .await
        .unwrap();
    let processing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM metrics_processing_state")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!((metrics, processing), (0, 0));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("export.toml")).unwrap(),
        "user export"
    );
    assert_eq!(core.factory_reset().await.unwrap(), 0);
}

#[tokio::test]
async fn reset_stops_on_token_deletion_failure_and_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = Arc::new(FailOnceTokenStore::default());
    let core = context(dir.path(), tokens.clone()).await;
    define_provider(&core, "ci-a").await;
    let id = core
        .provider_service
        .repository()
        .add_provider(&provider("ci-a"))
        .await
        .unwrap();
    tokens.fail_next_delete();

    assert!(core.factory_reset().await.is_err());
    assert!(core
        .provider_service
        .repository()
        .get_provider(id)
        .await
        .is_ok());
    assert!(core
        .config_state
        .load_providers()
        .await
        .unwrap()
        .contains_key("ci-a"));

    assert_eq!(core.factory_reset().await.unwrap(), 1);
    assert!(tokens.get_all_tokens().await.unwrap().is_empty());
}

#[tokio::test]
async fn reset_reports_cache_and_metrics_failures_and_supports_retry() {
    for table in ["workflow_parameters_cache", "pipeline_metrics"] {
        let dir = tempfile::tempdir().unwrap();
        let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
        let pool = core
            .provider_service
            .repository()
            .cache_pool()
            .as_sqlite()
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE {table} RENAME TO retained_data"
        )))
        .execute(pool)
        .await
        .unwrap();

        assert!(core.factory_reset().await.is_err(), "{table}");

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE retained_data RENAME TO {table}"
        )))
        .execute(pool)
        .await
        .unwrap();
        assert_eq!(core.factory_reset().await.unwrap(), 0);
    }
}

#[tokio::test]
async fn revoking_a_device_removes_its_push_registration() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    let (device, _) = core.pair_device("Pixel", "android").await.unwrap();
    core.device_pushes().set(&device.id, "sub-1").await.unwrap();

    assert!(core.revoke_device(&device.id).await.unwrap());

    assert_eq!(core.device_pushes().get(&device.id).await.unwrap(), None);
}

#[tokio::test]
async fn factory_reset_clears_alerts_and_push_registrations() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    core.device_alerts()
        .insert(&domain::NewDeviceAlert {
            key: "pipeline:p".into(),
            kind: domain::AlertKind::Failed,
            title: "t".into(),
            body: "b".into(),
            pipeline_id: None,
            provider_id: None,
            provider_ids: Vec::new(),
            finish: None,
        })
        .await
        .unwrap();
    core.device_pushes().set("d1", "sub-1").await.unwrap();

    core.factory_reset().await.unwrap();

    assert!(core.device_alerts().list().await.unwrap().is_empty());
    assert!(core.device_pushes().list().await.unwrap().is_empty());
}

#[tokio::test]
async fn factory_reset_forgets_every_device_and_push_and_announces_the_revocations() {
    let dir = tempfile::tempdir().unwrap();
    let bus = Arc::new(RecordingBus::default());
    let core = context_with_bus(dir.path(), Arc::new(MemoryTokenStore::new()), bus.clone()).await;
    let (phone, _) = core.pair_device("Pixel", "android").await.unwrap();
    let (tablet, _) = core.pair_device("iPad", "ios").await.unwrap();
    core.device_pushes().set(&phone.id, "sub-1").await.unwrap();
    let relay = PushRepository::new(core.provider_service.repository().cache_pool().clone());
    relay
        .import(&[crate::domain::PushSubscription {
            id: crate::domain::PushSubscription::local_id("relay-a"),
            relay_subscription_id: "relay-a".into(),
            device_name: "Pixel".into(),
            platform: "android".into(),
            created_at: chrono::Utc::now(),
            last_success_at: None,
        }])
        .await
        .unwrap();

    core.factory_reset().await.unwrap();

    assert!(core.devices().list().await.unwrap().is_empty());
    assert!(core.device_pushes().list().await.unwrap().is_empty());
    assert!(relay.list().await.unwrap().is_empty());
    let mut revoked: Vec<String> = bus
        .events()
        .into_iter()
        .filter_map(|event| match event {
            CoreEvent::DeviceRevoked { id } => Some(id),
            _ => None,
        })
        .collect();
    revoked.sort();
    let mut expected = vec![phone.id, tablet.id];
    expected.sort();
    assert_eq!(revoked, expected);
}

#[tokio::test]
async fn shutdown_stops_the_alert_recorder() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    assert!(!core.alert_stop.is_cancelled());

    core.shutdown().await;

    assert!(core.alert_stop.is_cancelled());
}
