use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Komodo".to_string(),
        provider_type: "komodo".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and deploy Komodo Stacks".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/komodo.svg".to_string()),
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
            section: ConfigSection::Connection,
            label: "Komodo Core URL".to_string(),
            description: Some(
                "URL of your Komodo Core (e.g., https://komodo.example.com)".to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: Some(r"^https?://.*".to_string()),
            validation_message: Some("Must be a valid HTTP or HTTPS URL".to_string()),
        })
        .add_field(ConfigField {
            key: "api_key".to_string(),
            section: ConfigSection::Authentication,
            label: "API Key".to_string(),
            description: Some(
                "Komodo API key, created under Profile > API Keys. Needs read access to the \
                 stacks, which also covers their update logs, and execute access to deploy them"
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
            section: ConfigSection::Authentication,
            label: "API Secret".to_string(),
            description: Some("Secret that belongs to the API key".to_string()),
            field_type: ConfigFieldType::Password,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
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
        cancel: false,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
        retains_run_history: false,
        logs: true,
        sign_in: false,
    }
}
