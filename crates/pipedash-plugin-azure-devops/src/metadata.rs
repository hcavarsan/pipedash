use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Azure DevOps",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities::standard(),
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("server_url", ConfigSection::Connection, "Organization URL")
                .description(
                    "URL of your Azure DevOps organization (e.g., https://dev.azure.com/my-org) or of an Azure DevOps Server collection (e.g., https://tfs.example.com/DefaultCollection)",
                )
                .required()
                .validation(r"^https?://[^\s?#]+$", "Must be a valid HTTP or HTTPS URL"),
        )
        .add_field(ConfigField::token(
            "Personal Access Token",
            "Azure DevOps personal access token with the scopes Build (Read & execute) and Project and Team (Read)",
        ))
        .add_field(status_page_field())
}
