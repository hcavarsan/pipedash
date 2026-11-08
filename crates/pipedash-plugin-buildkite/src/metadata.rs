use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Buildkite",
        ConfigSchema::new().add_field(ConfigField::token(
            "API Token",
            "Buildkite API Access Token with read_builds, read_build_logs, read_pipelines, and read_organizations scopes",
        )),
        schema::create_table_schema(),
        PluginCapabilities {
            ..PluginCapabilities::standard()
        },
    )
}
