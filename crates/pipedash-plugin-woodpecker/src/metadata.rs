use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Woodpecker CI".to_string(),
        provider_type: config::PROVIDER_TYPE.to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger Woodpecker CI pipelines".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/woodpecker.svg".to_string()),
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
            section: ConfigSection::Connection,
            label: "Woodpecker URL".to_string(),
            description: Some(
                "URL of your Woodpecker server (e.g., https://ci.example.com)".to_string(),
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
            section: ConfigSection::Authentication,
            label: "Personal Access Token".to_string(),
            description: Some(
                "Personal access token from your Woodpecker user settings. Starting and cancelling pipelines needs push access to the repository"
                    .to_string(),
            ),
            field_type: ConfigFieldType::Password,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(pipedash_plugin_api::status_page_field())
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
