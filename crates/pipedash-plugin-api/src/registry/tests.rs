use std::collections::HashMap;

use super::*;
use crate::testing::{
    manifest,
    Stub,
};
use crate::PluginCapabilities;

fn plugin() -> Box<dyn Plugin> {
    Box::new(Stub::new(PluginCapabilities::default()))
}

fn self_describing() -> Box<dyn Plugin> {
    let mut metadata = Stub::new(PluginCapabilities::standard()).metadata().clone();
    metadata.name = "Own name".to_string();
    metadata.provider_type = "own-type".to_string();
    metadata.version = "own-version".to_string();
    metadata.description = "own description".to_string();
    metadata.author = Some("own author".to_string());
    metadata.icon = Some("own icon".to_string());
    Box::new(Stub::with_metadata(metadata))
}

fn registered() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    registry.register(plugin, manifest("alpha"));
    registry.register(plugin, manifest("beta"));
    registry
}

#[test]
fn test_registry_creation() {
    let registry = PluginRegistry::new();
    assert_eq!(registry.count(), 0);
}

#[tokio::test]
async fn create_returns_separate_instances() {
    let registry = registered();

    let mut first = registry.create("alpha").expect("registered");
    let second = registry.create("alpha").expect("registered");
    first
        .initialize(1, HashMap::new(), None)
        .expect("initialize");

    assert!(first.validate_credentials().await.expect("first"));
    assert!(!second.validate_credentials().await.expect("second"));
    assert!(registry.create("missing").is_none());
}

#[test]
fn metadata_matches_the_registered_set() {
    let registry = registered();

    let mut from_metadata: Vec<String> = registry
        .metadata()
        .map(|metadata| metadata.provider_type.clone())
        .collect();
    let mut registered = registry.provider_types();
    from_metadata.sort();
    registered.sort();

    assert_eq!(from_metadata, ["alpha", "beta"]);
    assert_eq!(registered, from_metadata);
    assert_eq!(registry.count(), 2);
    assert!(registry.is_registered("alpha"));
    assert!(registry.get("beta").is_some());
    assert!(!registry.is_registered("gamma"));
}

#[test]
fn manifest_fields_override_the_plugins_own_values() {
    let mut registry = PluginRegistry::new();
    registry.register(
        self_describing,
        PluginManifest {
            package_name: "pipedash-plugin-merged",
            ..manifest("ignored")
        },
    );
    let merged = registry.metadata_of("merged").expect("keyed by manifest");

    assert_eq!(merged.provider_type, "merged");
    assert_eq!(merged.version, "1.2.3");
    assert_eq!(merged.description, "Stub provider");
    assert_eq!(merged.author.as_deref(), Some("Stub Team"));
    assert_eq!(
        merged.icon.as_deref(),
        Some("data:image/svg+xml,%3Csvg/%3E")
    );
    assert_eq!(merged.name, "Own name");
    assert!(merged.capabilities.trigger);
    assert_eq!(registry.metadata().count(), 1);
    assert!(registry.metadata_of("own-type").is_none());
    assert!(registry.metadata_of("missing").is_none());
}
