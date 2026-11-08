#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use pipedash_plugin_api::PluginError;

fn declared_providers() -> BTreeSet<String> {
    let manifest = include_str!("../Cargo.toml");
    let mut in_dependencies = false;
    let mut providers = BTreeSet::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_dependencies = line == "[dependencies]";
            continue;
        }
        let name = line.split(['=', '.']).next().unwrap_or_default().trim();
        if in_dependencies && name.starts_with("pipedash-plugin-") {
            providers.insert(name.trim_start_matches("pipedash-plugin-").to_string());
        }
    }
    providers.remove("api");
    providers.remove("actions");
    providers
}

#[test]
fn every_declared_provider_crate_is_registered_under_its_crate_name() {
    let registry = pipedash_plugins::registry();
    let types: BTreeSet<_> = registry.provider_types().into_iter().collect();

    assert!(!types.is_empty());
    assert_eq!(types, declared_providers());
    assert_eq!(registry.count(), types.len());
}

#[test]
fn metadata_matches_the_registered_provider_types() {
    let registry = pipedash_plugins::registry();
    let from_metadata: BTreeSet<_> = registry
        .metadata()
        .map(|metadata| metadata.provider_type.clone())
        .collect();
    let types: BTreeSet<_> = registry.provider_types().into_iter().collect();

    assert_eq!(registry.metadata().count(), types.len());
    assert_eq!(from_metadata, types);
}

#[test]
fn every_plugin_ships_self_contained_metadata() {
    let registry = pipedash_plugins::registry();
    for metadata in registry.metadata() {
        let provider = &metadata.provider_type;
        assert!(!metadata.name.trim().is_empty(), "{provider}");
        let icon = metadata.icon.as_deref().unwrap_or_default();
        assert!(icon.starts_with("data:image/svg+xml,"), "{provider}");
        assert!(!metadata.table_schema.tables.is_empty(), "{provider}");
        let keys: BTreeSet<_> = metadata
            .config_schema
            .fields
            .iter()
            .map(|field| &field.key)
            .collect();
        assert_eq!(
            keys.len(),
            metadata.config_schema.fields.len(),
            "{provider}"
        );
        let terminology = &metadata.presentation.terminology;
        assert!(
            !terminology.item.is_empty() && !terminology.run.is_empty(),
            "{provider}"
        );
        assert!(
            !metadata.presentation.trigger.label.is_empty(),
            "{provider}"
        );
        assert!(
            !metadata.presentation.status_options.is_empty(),
            "{provider}"
        );
    }
}

#[test]
fn create_returns_a_separate_instance_for_every_call() {
    let registry = pipedash_plugins::registry();

    for provider_type in registry.provider_types() {
        let first = registry.create(&provider_type).unwrap();
        let second = registry.create(&provider_type).unwrap();
        assert_ne!(
            std::ptr::from_ref(&*first).cast::<()>(),
            std::ptr::from_ref(&*second).cast::<()>(),
            "{provider_type}"
        );
    }
    assert!(registry.create("unknown").is_none());
}

#[tokio::test]
async fn plugins_without_force_cancel_reject_it() {
    let registry = pipedash_plugins::registry();
    for metadata in registry.metadata() {
        if metadata.capabilities.force_cancel {
            continue;
        }
        let plugin = registry.create(&metadata.provider_type).unwrap();
        assert!(matches!(
            plugin.force_cancel_run("unused", 7).await,
            Err(PluginError::NotSupported(_))
        ));
    }
}
