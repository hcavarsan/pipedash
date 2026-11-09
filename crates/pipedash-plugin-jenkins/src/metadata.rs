use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Jenkins",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities::standard(),
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text(
                "server_url",
                ConfigSection::Connection,
                "Jenkins Server URL",
            )
            .description("Your Jenkins server URL (e.g., https://jenkins.example.com)")
            .required(),
        )
        .add_field(
            ConfigField::text("username", ConfigSection::Authentication, "Username")
                .description("Your Jenkins username")
                .required(),
        )
        .add_field(ConfigField::token(
            "API Token",
            "Jenkins API token (not your password)",
        ))
        .add_field(
            ConfigField::boolean(
                "follow_all_jobs",
                ConfigSection::Settings,
                "Follow all jobs",
            )
            .description(
                "Monitor every supported job you can read. Turn off to restore manual selection.",
            )
            .default_bool(false),
        )
        .add_field(status_page_field())
}
