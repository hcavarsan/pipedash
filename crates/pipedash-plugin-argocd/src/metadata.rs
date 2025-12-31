use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "ArgoCD".to_string(),
        provider_type: "argocd".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and manage ArgoCD applications and deployments".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://cdn.simpleicons.org/argo/EF7B4D".to_string()),
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
            label: "ArgoCD Server URL".to_string(),
            description: Some(
                "ArgoCD API server URL (e.g., https://argocd.example.com)".to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: Some(r"^https?://.*".to_string()),
            validation_message: Some("Must be a valid HTTP or HTTPS URL".to_string()),
        })
        .add_field(ConfigField {
            key: "token".to_string(),
            label: "Authentication Token".to_string(),
            description: Some(
                "ArgoCD authentication token or Kubernetes service account token".to_string(),
            ),
            field_type: ConfigFieldType::Password,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "insecure".to_string(),
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
        .add_field(ConfigField {
            key: "organizations".to_string(),
            label: "Git Organizations Filter (optional)".to_string(),
            description: Some(
                "Select Git organizations to filter applications (leave empty to show all)"
                    .to_string(),
            ),
            field_type: ConfigFieldType::MultiSelect,
            required: false,
            default_value: None,
            options: None, // Will be populated dynamically via get_field_options
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
