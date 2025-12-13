use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Bitbucket Pipelines".to_string(),
        provider_type: "bitbucket".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger Bitbucket Cloud Pipelines CI/CD".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://cdn.simpleicons.org/bitbucket/0052CC".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: create_required_permissions(),
        features: create_features(),
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(ConfigField {
            key: "email".to_string(),
            label: "Email".to_string(),
            description: Some(
                "Your Atlassian account email (found in Bitbucket Personal settings > Email)"
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
            key: "api_token".to_string(),
            label: "API Token".to_string(),
            description: Some(
                "Bitbucket API token with Repository:Read, Workspace:Read, and Pipelines:Read/Write scopes"
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
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
    }
}

fn create_required_permissions() -> Vec<Permission> {
    vec![
        Permission {
            name: "read:user:bitbucket".to_string(),
            description: "Read current user information (for credential validation)".to_string(),
            required: true,
        },
        Permission {
            name: "read:repository:bitbucket".to_string(),
            description: "Read repository information and source code access".to_string(),
            required: true,
        },
        Permission {
            name: "read:workspace:bitbucket".to_string(),
            description: "Read workspace and workspace permission data".to_string(),
            required: true,
        },
        Permission {
            name: "read:pipeline:bitbucket".to_string(),
            description: "Read pipeline runs, steps, logs, and status".to_string(),
            required: true,
        },
        Permission {
            name: "write:pipeline:bitbucket".to_string(),
            description: "Trigger and cancel pipeline runs".to_string(),
            required: false,
        },
    ]
}

fn create_features() -> Vec<Feature> {
    vec![
        Feature {
            id: "view_pipelines".to_string(),
            name: "View Pipelines".to_string(),
            description: "View pipeline runs and status".to_string(),
            required_permissions: vec![
                "read:repository:bitbucket".to_string(),
                "read:workspace:bitbucket".to_string(),
                "read:pipeline:bitbucket".to_string(),
            ],
        },
        Feature {
            id: "trigger_pipelines".to_string(),
            name: "Trigger Pipelines".to_string(),
            description: "Start new pipeline runs".to_string(),
            required_permissions: vec!["write:pipeline:bitbucket".to_string()],
        },
        Feature {
            id: "cancel_pipelines".to_string(),
            name: "Cancel Pipelines".to_string(),
            description: "Stop running pipelines".to_string(),
            required_permissions: vec!["write:pipeline:bitbucket".to_string()],
        },
    ]
}
