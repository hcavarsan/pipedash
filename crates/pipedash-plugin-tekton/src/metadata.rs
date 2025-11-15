//! Tekton CD plugin metadata
//!
//! This module contains plugin metadata, configuration schema, and
//! capabilities.

use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

/// Validates comma-separated K8s namespace names
const NAMESPACE_LIST_REGEX: &str =
    r"^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)(\s*,\s*[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$";

/// Creates the plugin metadata for Tekton CD
///
/// This includes:
/// - Basic plugin information (name, version, description)
/// - Configuration schema (kubeconfig path and context)
/// - Table schema (from schema module)
/// - Plugin capabilities
pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Tekton CD".to_string(),
        provider_type: "tekton".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger Tekton CI/CD pipelines running on Kubernetes".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://cdn.simpleicons.org/tekton/FD495C".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: Vec::new(),
        features: Vec::new(),
    }
}

/// Creates the configuration schema for Tekton CD
///
/// Defines two fields:
/// - `kubeconfig_path`: Path to Kubernetes config file(s) (optional, uses
///   $KUBECONFIG)
/// - `context`: Kubernetes context to use (optional, uses current-context)
fn create_config_schema() -> ConfigSchema {
    let default_kubeconfig = config::get_default_kubeconfig_path();

    ConfigSchema::new()
        .add_field(ConfigField {
            key: "kubeconfig_path".to_string(),
            label: "Kubeconfig Path".to_string(),
            description: Some(
                "Path to your Kubernetes config file(s). Multiple paths can be separated by ':' (Unix) or ';' (Windows). Uses $KUBECONFIG env var if set."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: Some(serde_json::Value::String(default_kubeconfig)),
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "context".to_string(),
            label: "Kubernetes Context".to_string(),
            description: Some(
                "Select a context from your kubeconfig. Leave empty to use current-context."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Select,
            required: false,
            default_value: None,
            options: Some(Vec::new()),
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "namespace_mode".to_string(),
            label: "Namespace Discovery Mode".to_string(),
            description: Some(
                "How to discover namespaces containing Tekton pipelines:\n• 'all' - Automatically discover all namespaces (requires cluster-wide namespace list permissions)\n• 'custom' - Manually specify namespaces (recommended for users without admin permissions or on OpenShift/RHOS)\n\nIf you see a '403 Forbidden' error during validation, switch to 'custom' mode and specify namespaces manually."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Select,
            required: false,
            default_value: Some(serde_json::Value::String("all".to_string())),
            options: Some(vec![
                "all".to_string(),
                "custom".to_string(),
            ]),
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "namespaces".to_string(),
            label: "Namespaces".to_string(),
            description: Some(
                "Comma-separated list of namespaces to monitor (e.g., 'default,prod,staging'). Required when namespace mode is 'custom'. Leave empty when mode is 'all' to auto-discover. Each namespace must be a valid Kubernetes namespace name (lowercase alphanumeric, hyphens, max 63 chars)."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: None,
            options: None,
            validation_regex: Some(NAMESPACE_LIST_REGEX.to_string()),
            validation_message: Some(
                "Invalid namespace format. Each namespace must: start and end with alphanumeric, contain only lowercase letters, numbers, and hyphens, be max 63 characters. Separate multiple namespaces with commas.".to_string(),
            ),
        })
}

/// Creates the plugin capabilities
///
/// Tekton supports:
/// - Pipelines
/// - Pipeline runs
/// - Triggering pipeline runs
fn create_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        pipelines: true,
        pipeline_runs: true,
        trigger: true,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
    }
}
