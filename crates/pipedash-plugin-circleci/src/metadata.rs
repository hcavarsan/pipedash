use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "CircleCI".to_string(),
        provider_type: "circleci".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and run CircleCI pipelines".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/circleci.svg".to_string()),
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
            key: "token".to_string(),
            section: ConfigSection::Authentication,
            label: "Personal API Token".to_string(),
            description: Some(
                "CircleCI personal API token, created under User Settings > Personal API Tokens. \
                 It reads and runs pipelines in every project the user can access"
                    .to_string(),
            ),
            field_type: ConfigFieldType::Password,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "base_url".to_string(),
            section: ConfigSection::Connection,
            label: "CircleCI URL".to_string(),
            description: Some(
                "Leave as https://circleci.com for CircleCI cloud, or use the address of your \
                 CircleCI server"
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: Some(serde_json::Value::String(
                config::DEFAULT_BASE_URL.to_string(),
            )),
            options: None,
            validation_regex: Some(r"^https?://[^/?#]+/?$".to_string()),
            validation_message: Some("Must be an HTTP or HTTPS address without a path".to_string()),
        })
        .add_field(ConfigField {
            key: "insecure".to_string(),
            section: ConfigSection::Connection,
            label: "Skip TLS Verification".to_string(),
            description: Some(
                "Skip TLS certificate verification (useful for self-signed certificates)"
                    .to_string(),
            ),
            field_type: ConfigFieldType::Boolean,
            required: false,
            default_value: Some(serde_json::Value::Bool(false)),
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
        force_cancel: false,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
        retains_run_history: false,
        logs: true,
        sign_in: false,
    }
}
