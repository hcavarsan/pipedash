use pipedash_plugin_api::defaults::default_pipelines_table;

use super::*;
use crate::application::test_support::Harness;

#[test]
fn defaults_follow_the_table_columns() {
    let table = default_pipelines_table();
    let preferences = default_preferences(&table);

    assert_eq!(
        preferences.column_order,
        table
            .columns
            .iter()
            .map(|col| col.id.clone())
            .collect::<Vec<_>>()
    );
    for col in &table.columns {
        assert_eq!(preferences.column_visibility[&col.id], col.default_visible);
    }
}

#[test]
fn defaults_keep_the_wire_names() {
    let json = serde_json::to_value(default_preferences(&default_pipelines_table())).unwrap();

    assert!(json.get("columnOrder").is_some());
    assert!(json.get("columnVisibility").is_some());
}

#[tokio::test]
async fn an_unknown_table_has_no_defaults() {
    let h = Harness::new().await;
    let id = h.add_provider("one").await;

    let error = h
        .provider_service
        .default_table_preferences(id, "missing")
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::NotFound(_)));
}

#[tokio::test]
async fn saved_preferences_round_trip() {
    let h = Harness::new().await;
    let id = h.add_provider("one").await;

    assert_eq!(
        h.provider_service.table_preferences(id, "t").await.unwrap(),
        None
    );
    h.provider_service
        .save_table_preferences(id, "t", "{\"a\":1}")
        .await
        .unwrap();

    assert_eq!(
        h.provider_service
            .table_preferences(id, "t")
            .await
            .unwrap()
            .as_deref(),
        Some("{\"a\":1}")
    );
}
