use pipedash_plugin_api::*;

use crate::schema;

const HOST_REGEX: &str = r"^[A-Za-z0-9][A-Za-z0-9.-]*(:[0-9]{1,5})?$";

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Container registry",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities {
            uses_http_proxy: true,
            ..PluginCapabilities::default()
        },
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("registry_host", ConfigSection::Connection, "Registry host")
                .description(
                    "Host of the registry, for example ghcr.io or registry.example.com:5000",
                )
                .required()
                .validation(HOST_REGEX, "Enter a host name without a scheme or path"),
        )
        .add_field(
            ConfigField::text("username", ConfigSection::Authentication, "Username")
                .description("Leave empty when the token is a bearer token instead of a password"),
        )
        .add_field(
            ConfigField::text("token_host", ConfigSection::Authentication, "Token host")
                .description(
                    "Host that issues registry tokens when it differs from the registry, for \
                     example auth.example.com. It receives the username and password; leave \
                     empty to request tokens anonymously",
                )
                .validation(HOST_REGEX, "Enter a host name without a scheme or path"),
        )
        .add_field(ConfigField::token(
            "Password or Bearer Token",
            "Registry password, or a bearer token when no username is set",
        ))
}
