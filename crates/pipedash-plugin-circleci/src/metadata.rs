use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "CircleCI",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities::standard(),
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(ConfigField::token(
            "Personal API Token",
            "CircleCI personal API token, created under User Settings > Personal API Tokens. \
             It reads and runs pipelines in every project the user can access",
        ))
        .add_field(
            ConfigField::text("base_url", ConfigSection::Connection, "CircleCI URL")
                .description(
                    "Leave as https://circleci.com for CircleCI cloud, or use the address of your \
                     CircleCI server",
                )
                .default_text(config::DEFAULT_BASE_URL)
                .validation(
                    r"^https?://[^/?#]+/?$",
                    "Must be an HTTP or HTTPS address without a path",
                ),
        )
        .add_field(ConfigField::insecure_tls())
}
