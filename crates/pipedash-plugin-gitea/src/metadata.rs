use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Gitea Actions".to_string(),
        provider_type: "gitea".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger Gitea Actions workflows".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/gitea.svg".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: Vec::new(),
        features: Vec::new(),
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(ConfigField {
            key: "base_url".to_string(),
            label: "Gitea Base URL".to_string(),
            description: Some(
                "URL of your Gitea instance (e.g., https://gitea.example.com). Requires Gitea 1.25 or newer"
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "token".to_string(),
            label: "Access Token".to_string(),
            description: Some(
                "Gitea access token with read:user, read:organization and read/write:repository scopes"
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

fn create_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        pipelines: true,
        pipeline_runs: true,
        trigger: true,
        cancel: true,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
        retains_run_history: false,
    }
}
