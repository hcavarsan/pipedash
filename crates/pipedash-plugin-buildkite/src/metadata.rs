use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Buildkite".to_string(),
        provider_type: "buildkite".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor Buildkite builds, agents, and artifacts".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://cdn.simpleicons.org/buildkite/14CC80".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: Vec::new(),
        features: Vec::new(),
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new().add_field(ConfigField {
        key: "token".to_string(),
        label: "API Token".to_string(),
        description: Some(
            "Buildkite API Access Token with read_builds, read_pipelines, and read_organizations scopes"
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
        agents: true,
        artifacts: true,
        queues: false,
        custom_tables: false,
    }
}
