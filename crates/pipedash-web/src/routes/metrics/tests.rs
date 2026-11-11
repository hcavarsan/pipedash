use std::sync::Arc;

use axum::body::Body;
use axum::http::{
    header,
    Request,
    StatusCode,
};
use pipedash_core::domain::{
    MetricEntry,
    MetricType,
};
use pipedash_core::infrastructure::database::MetricsRepository;
use pipedash_core::infrastructure::token_store::MemoryTokenStore;
use pipedash_core::infrastructure::{
    ConfigLoader,
    StorageManager,
};
use pipedash_core::CoreContext;
use serde_json::json;
use tower::ServiceExt;

use super::*;
use crate::ws::event_bus::WebSocketEventBus;

async fn query(
    app: &Router, uri: &str, body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

#[tokio::test]
async fn dashboard_rejects_invalid_values_and_reports_observed_metrics() {
    const CHILD: &str = "PIPEDASH_METRICS_FILTER_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let root =
            std::env::temp_dir().join(format!("pipedash-metrics-filters-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let config_path = root.join("config.toml");
        std::fs::write(&config_path, format!(
            "[general]\nmetrics_enabled = true\nstatus_pages_enabled = false\n[storage]\nbackend = \"sqlite\"\ndata_dir = {:?}\n",
            root.to_string_lossy()
        )).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "routes::metrics::tests::dashboard_rejects_invalid_values_and_reports_observed_metrics", "--nocapture"])
            .env(CHILD, "1")
            .env("PIPEDASH_CONFIG_PATH", config_path)
            .output().unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let config = ConfigLoader::load(&ConfigLoader::discover_config_path()).unwrap();
    let manager = StorageManager::with_token_store(config, Arc::new(MemoryTokenStore::new()), true)
        .await
        .unwrap();
    let bus = Arc::new(WebSocketEventBus::new());
    let core =
        CoreContext::with_storage_manager(&manager, bus.clone(), crate::test_support::registry())
            .await
            .unwrap();
    let repository = MetricsRepository::new_from_pool(manager.database_pool());
    let timestamp = chrono::Utc::now() - chrono::Duration::minutes(1);
    let entries = [
        (MetricType::RunDuration, 60.0, "duration"),
        (MetricType::SuccessRate, 100.0, "success"),
    ]
    .into_iter()
    .map(|(metric_type, value, hash)| MetricEntry {
        id: 0,
        pipeline_id: "example".to_string(),
        run_number: 1,
        timestamp,
        metric_type,
        value,
        metadata: None,
        created_at: timestamp,
        run_hash: Some(hash.to_string()),
    })
    .collect();
    repository.insert_metrics_batch(entries).await.unwrap();
    let state = AppState::initialized(bus, core.clone(), manager, crate::test_support::registry());
    let app = router().with_state(state);

    let base = json!({
        "pipeline_id": "example",
        "metric_type": "run_duration",
        "aggregation_period": "daily",
        "date_range": "24h",
    });

    let (status, result) = query(&app, "/dashboard", base.clone()).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let metrics = result["data"]["metrics"].as_array().unwrap();
    assert!(metrics
        .iter()
        .any(|metric| metric["value"] == 60.0 && metric["count"] == 1));
    assert!(result["observed"]
        .as_array()
        .unwrap()
        .contains(&json!(true)));

    for (field, value) in [
        ("metric_type", "run_duraton"),
        ("date_range", "yesterday"),
        ("aggregation_period", ""),
    ] {
        let mut body = base.clone();
        body[field] = json!(value);
        let (status, _) = query(&app, "/dashboard", body).await;
        assert!(status.is_client_error(), "{field}: {status}");
    }
    core.shutdown().await;
}
