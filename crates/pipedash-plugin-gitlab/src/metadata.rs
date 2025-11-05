//! GitLab CI plugin metadata
//!
//! This module contains plugin metadata, configuration schema, and
//! capabilities.

use pipedash_plugin_api::*;

use crate::schema;

/// Creates the plugin metadata for GitLab CI
///
/// This includes:
/// - Basic plugin information (name, version, description)
/// - Configuration schema (API token and optional base URL)
/// - Table schema (from schema module)
/// - Plugin capabilities
pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "GitLab CI".to_string(),
        provider_type: "gitlab".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger GitLab CI/CD pipelines".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://cdn.simpleicons.org/gitlab/FC6D26".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
    }
}

/// Creates the configuration schema for GitLab CI
///
/// Defines two fields:
/// - `token`: GitLab Personal Access Token (required)
/// - `base_url`: GitLab instance URL (optional, for self-hosted instances)
fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(ConfigField {
            key: "token".to_string(),
            label: "API Token".to_string(),
            description: Some("GitLab Personal Access Token with api scope".to_string()),
            field_type: ConfigFieldType::Password,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "base_url".to_string(),
            label: "GitLab Base URL (optional)".to_string(),
            description: Some(
                "Leave empty for GitLab.com, or enter your self-hosted GitLab URL (e.g., https://gitlab.example.com)"
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
}

/// Creates the plugin capabilities
///
/// GitLab CI supports:
/// - Pipelines
/// - Pipeline runs
/// - Triggering pipelines
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
