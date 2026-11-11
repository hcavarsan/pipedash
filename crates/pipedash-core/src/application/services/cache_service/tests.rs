use std::sync::Arc;

use super::*;
use crate::application::test_support::plugins::registry;
use crate::event::NoOpEventBus;
use crate::infrastructure::{
    MemoryTokenStore,
    PipedashConfig,
    StorageManager,
};
use crate::test_env::EnvGuard;

async fn context(dir: &std::path::Path) -> CoreContext {
    let config_path = dir.join("config.toml");
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.display().to_string();
    config.general.metrics_enabled = false;
    let manager = StorageManager::with_token_store(config, Arc::new(MemoryTokenStore::new()), true)
        .await
        .unwrap();

    CoreContext::with_storage_manager(&manager, Arc::new(NoOpEventBus), registry())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_fresh_core_reports_empty_caches() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path()).await;

    let stats = core.cache_stats().await.unwrap();

    assert_eq!(
        stats,
        CacheStats {
            pipelines_count: 0,
            run_history_count: 0,
            workflow_params_count: 0,
            metrics_count: 0,
            run_logs_bytes: 0,
        }
    );
    core.shutdown().await;
}

#[tokio::test]
async fn clearing_all_caches_keeps_the_stats_empty() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path()).await;

    core.clear_all_caches().await.unwrap();

    assert_eq!(core.cache_stats().await.unwrap().run_logs_bytes, 0);
    core.shutdown().await;
}

#[tokio::test]
async fn a_core_under_test_keeps_its_config_in_its_own_directory() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path()).await;

    assert_eq!(
        core.config_state.config_path(),
        &dir.path().join("config.toml")
    );
    core.shutdown().await;
}

#[test]
fn cache_stats_keep_the_wire_field_names() {
    let json = serde_json::to_value(CacheStats {
        pipelines_count: 1,
        run_history_count: 2,
        workflow_params_count: 3,
        metrics_count: 4,
        run_logs_bytes: 5,
    })
    .unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "pipelines_count": 1,
            "run_history_count": 2,
            "workflow_params_count": 3,
            "metrics_count": 4,
            "run_logs_bytes": 5,
        })
    );
}
