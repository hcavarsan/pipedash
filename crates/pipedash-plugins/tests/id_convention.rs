#![allow(clippy::unwrap_used, clippy::expect_used)]

use pipedash_core::domain::{
    DomainError,
    Provider,
};
use pipedash_core::infrastructure::providers::PluginAdapter;

const PROVIDER_ID: i64 = 42;
const LOCALS: [&str; 4] = ["a", "a__b", "owner/repo/ci.yml", "ns__kind__x__7"];

fn each_adapter(mut check: impl FnMut(&str, &PluginAdapter)) {
    let registry = pipedash_plugins::registry();
    for metadata in registry.metadata() {
        let provider_type = metadata.provider_type.clone();
        let plugin = registry.create(&provider_type).unwrap();
        let adapter = PluginAdapter::new(plugin, provider_type.clone(), PROVIDER_ID, None);
        check(&provider_type, &adapter);
    }
}

#[test]
fn every_plugin_adapter_round_trips_local_ids() {
    each_adapter(|provider_type, adapter| {
        for local in LOCALS {
            let global = adapter.global_id(local);
            assert_eq!(
                global,
                format!("{provider_type}__{PROVIDER_ID}__{local}"),
                "{provider_type}"
            );
            assert_eq!(adapter.local_id(&global).unwrap(), local, "{provider_type}");
        }
    });
}

#[test]
fn every_plugin_adapter_rejects_foreign_and_unprefixed_ids() {
    each_adapter(|provider_type, adapter| {
        let foreign = [
            format!("other__{PROVIDER_ID}__x"),
            format!("{provider_type}__{}__x", PROVIDER_ID + 1),
            format!("{provider_type}x__{PROVIDER_ID}__x"),
            format!("{provider_type}__{PROVIDER_ID}__"),
            "x".to_string(),
            String::new(),
        ];
        for id in &foreign {
            assert!(
                matches!(adapter.local_id(id), Err(DomainError::InvalidConfig(_))),
                "{provider_type}: {id}"
            );
        }
    });
}

#[tokio::test]
async fn every_plugin_adapter_rejects_foreign_ids_before_calling_the_plugin() {
    let registry = pipedash_plugins::registry();
    for metadata in registry.metadata() {
        let provider_type = metadata.provider_type.clone();
        let plugin = registry.create(&provider_type).unwrap();
        let adapter = PluginAdapter::new(plugin, provider_type.clone(), PROVIDER_ID, None);
        let foreign = format!("other__{PROVIDER_ID}__x");

        assert!(
            matches!(
                adapter.fetch_run_history(&foreign, 1).await,
                Err(DomainError::InvalidConfig(_))
            ),
            "{provider_type}"
        );
        assert!(
            matches!(
                adapter.cancel_run(&foreign, 1).await,
                Err(DomainError::InvalidConfig(_))
            ),
            "{provider_type}"
        );
        assert!(
            matches!(
                adapter.get_workflow_parameters(&foreign).await,
                Err(DomainError::InvalidConfig(_))
            ),
            "{provider_type}"
        );
    }
}
