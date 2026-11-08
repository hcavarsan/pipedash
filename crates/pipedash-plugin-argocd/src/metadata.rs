use pipedash_plugin_api::*;

use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "ArgoCD",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities {
            sign_in: true,
            ..PluginCapabilities::standard()
        },
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text("server_url", ConfigSection::Connection, "ArgoCD Server URL")
                .description("ArgoCD API server URL (e.g., https://argocd.example.com)")
                .required()
                .http_url_validation(),
        )
        .add_field(ConfigField::insecure_tls())
        .add_field(ConfigField::token(
            "Authentication Token",
            "ArgoCD authentication token or Kubernetes service account token",
        ))
        .add_field(status_page_field())
}
