use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Jenkins".to_string(),
        provider_type: "jenkins".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor Jenkins jobs, builds, and pipelines".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://www.jenkins.io/favicon.ico".to_string()),
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
            label: "Jenkins Server URL".to_string(),
            description: Some(
                "Your Jenkins server URL (e.g., https://jenkins.example.com)".to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "username".to_string(),
            label: "Username".to_string(),
            description: Some("Your Jenkins username".to_string()),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "token".to_string(),
            label: "API Token".to_string(),
            description: Some("Jenkins API token (not your password)".to_string()),
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
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
    }
}
