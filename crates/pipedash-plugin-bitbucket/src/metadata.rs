use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        token_prefixes: vec!["ATBB".to_string()],
        ..PluginMetadata::new(
            "Bitbucket Pipelines",
            create_config_schema(),
            schema::create_table_schema(),
            PluginCapabilities::standard(),
        )
        .with_permissions(create_required_permissions())
        .with_features(create_features())
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("email", ConfigSection::Authentication, "Email")
                .description(
                    "Your Atlassian account email (found in Bitbucket Personal settings > Email)",
                )
                .required(),
        )
        .add_field(
            ConfigField::password("api_token", ConfigSection::Authentication, "API Token")
                .description(
                    "Bitbucket API token with Repository:Read, Workspace:Read, and Pipelines:Read/Write scopes",
                )
                .required(),
        )
}

fn create_required_permissions() -> Vec<Permission> {
    vec![
        Permission::new(
            "read:user:bitbucket",
            "Read current user information (for credential validation)",
            true,
        ),
        Permission::new(
            "read:repository:bitbucket",
            "Read repository information and source code access",
            true,
        ),
        Permission::new(
            "read:workspace:bitbucket",
            "Read workspace and workspace permission data",
            true,
        ),
        Permission::new(
            "read:pipeline:bitbucket",
            "Read pipeline runs, steps, logs, and status",
            true,
        ),
        Permission::new(
            "write:pipeline:bitbucket",
            "Trigger and cancel pipeline runs",
            false,
        ),
    ]
}

fn create_features() -> Vec<Feature> {
    vec![
        Feature::new(
            "view_pipelines",
            "View Pipelines",
            "View pipeline runs and status",
            &[
                "read:repository:bitbucket",
                "read:workspace:bitbucket",
                "read:pipeline:bitbucket",
            ],
        ),
        Feature::new(
            "trigger_pipelines",
            "Trigger Pipelines",
            "Start new pipeline runs",
            &["write:pipeline:bitbucket"],
        ),
        Feature::new(
            "cancel_pipelines",
            "Cancel Pipelines",
            "Stop running pipelines",
            &["write:pipeline:bitbucket"],
        ),
    ]
}
