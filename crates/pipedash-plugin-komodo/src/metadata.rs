use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Komodo",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities {
            cancel: false,
            ..PluginCapabilities::standard()
        },
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("server_url", ConfigSection::Connection, "Komodo Core URL")
                .description("URL of your Komodo Core (e.g., https://komodo.example.com)")
                .required()
                .http_url_validation(),
        )
        .add_field(
            ConfigField::text("api_key", ConfigSection::Authentication, "API Key")
                .description(
                    "Komodo API key, created under Profile > API Keys. Needs read access to the \
                     stacks, which also covers their update logs, and execute access to deploy them",
                )
                .required(),
        )
        .add_field(ConfigField::token(
            "API Secret",
            "Secret that belongs to the API key",
        ))
        .add_field(ConfigField::insecure_tls())
}
