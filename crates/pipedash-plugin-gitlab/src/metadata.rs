use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        token_prefixes: ["glpat-", "gldt-"].map(str::to_string).to_vec(),
        ..PluginMetadata::new(
            "GitLab CI",
            create_config_schema(),
            schema::create_table_schema(),
            PluginCapabilities::standard(),
        )
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(ConfigField::token(
            "API Token",
            "GitLab Personal Access Token with api scope",
        ))
        .add_field(
            ConfigField::text(
                "base_url",
                ConfigSection::Connection,
                "GitLab Base URL (optional)",
            )
            .description(
                "Leave empty for GitLab.com, or enter your self-hosted GitLab URL (e.g., https://gitlab.example.com)",
            ),
        )
        .add_field(status_page_field())
}
