use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Coolify",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities::standard(),
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("server_url", ConfigSection::Connection, "Coolify URL")
                .description(
                    "URL of your Coolify dashboard (e.g., https://coolify.example.com). API access \
                     must be enabled in the instance settings",
                )
                .required()
                .http_url_validation(),
        )
        .add_field(ConfigField::token(
            "API Token",
            "Coolify API token, created under Keys & Tokens > API Tokens. Needs read to list \
             applications and deployments and deploy to start or cancel them. Add \
             read:sensitive to see deployment logs; deploy and read:sensitive require the \
             token owner to be a team admin or owner",
        ))
        .add_field(ConfigField::insecure_tls())
}
