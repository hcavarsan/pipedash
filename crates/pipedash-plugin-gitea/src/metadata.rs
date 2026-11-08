use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Gitea Actions",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities::standard(),
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("base_url", ConfigSection::Connection, "Gitea Base URL")
                .description(
                    "URL of your Gitea instance (e.g., https://gitea.example.com). Requires Gitea 1.25 or newer",
                )
                .required(),
        )
        .add_field(ConfigField::token(
            "Access Token",
            "Gitea access token with read:user, read:organization and read/write:repository scopes",
        ))
        .add_field(status_page_field())
}
