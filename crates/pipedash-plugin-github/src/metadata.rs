//! GitHub Actions plugin metadata
//!
//! This module contains plugin metadata, configuration schema, and
//! capabilities.

use pipedash_plugin_api::*;

use crate::schema;

/// Creates the plugin metadata for GitHub Actions
///
/// This includes:
/// - Basic plugin information (name, version, description)
/// - Configuration schema (API token field)
/// - Table schema (from schema module)
/// - Plugin capabilities
pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "GitHub Actions".to_string(),
        provider_type: "github".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger GitHub Actions workflows".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://cdn.simpleicons.org/github/white".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: Vec::new(),
        features: Vec::new(),
    }
}

/// Creates the configuration schema for GitHub Actions
///
/// Defines a single required field:
/// - `token`: GitHub Personal Access Token or Fine-grained token
fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new().add_field(ConfigField {
        key: "token".to_string(),
        label: "API Token".to_string(),
        description: Some(
            "GitHub Personal Access Token or Fine-grained token with repo and workflow permissions"
                .to_string(),
        ),
        field_type: ConfigFieldType::Password,
        required: true,
        default_value: None,
        options: None,
        validation_regex: None,
        validation_message: None,
    })
}

/// Creates the plugin capabilities
///
/// GitHub Actions supports:
/// - Pipelines (workflows)
/// - Pipeline runs (workflow runs)
/// - Triggering workflows
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
