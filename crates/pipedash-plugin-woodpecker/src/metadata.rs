use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Woodpecker CI",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities::standard(),
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("base_url", ConfigSection::Connection, "Woodpecker URL")
                .description("URL of your Woodpecker server (e.g., https://ci.example.com)")
                .required(),
        )
        .add_field(ConfigField::token(
            "Personal Access Token",
            "Personal access token from your Woodpecker user settings. Starting and cancelling pipelines needs push access to the repository",
        ))
        .add_field(status_page_field())
}
