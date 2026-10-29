use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Azure DevOps".to_string(),
        provider_type: config::PROVIDER_TYPE.to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger Azure Pipelines (YAML and classic)".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/azure-devops.svg".to_string()),
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
            key: "server_url".to_string(),
            label: "Organization URL".to_string(),
            description: Some(
                "URL of your Azure DevOps organization (e.g., https://dev.azure.com/my-org) or of an Azure DevOps Server collection (e.g., https://tfs.example.com/DefaultCollection)"
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: Some(r"^https?://[^\s?#]+$".to_string()),
            validation_message: Some("Must be a valid HTTP or HTTPS URL".to_string()),
        })
        .add_field(ConfigField {
            key: "token".to_string(),
            label: "Personal Access Token".to_string(),
            description: Some(
                "Azure DevOps personal access token with the scopes Build (Read & execute) and Project and Team (Read)"
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
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
        retains_run_history: false,
        logs: false,
        sign_in: false,
    }
}
