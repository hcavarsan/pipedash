use pipedash_plugin_api::*;

use crate::config;
use crate::schema;

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Argo Workflows",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities {
            trigger: false,
            cancel: false,
            logs: false,
            uses_http_proxy: false,
            ..PluginCapabilities::standard()
        },
    )
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text(
                config::KUBECONFIG_PATH_KEY,
                ConfigSection::Authentication,
                "Kubeconfig Path",
            )
            .description(
                "Kubeconfig file(s) on the machine running Pipedash, separated by ':' (';' on Windows). Ignored when Kubeconfig Content is set.",
            )
            .default_text(&kubernetes::default_kubeconfig_path())
            .scope(ConfigScope::Cluster),
        )
        .add_field(ConfigField::kubeconfig_content(
            kubernetes::KUBECONFIG_CONTENT_KEY,
            "Kubeconfig Content",
        ))
        .add_field(
            ConfigField::select(
                config::CONTEXT_KEY,
                ConfigSection::Authentication,
                "Kubernetes Context",
                &[],
            )
            .description("Context from your kubeconfig. Leave empty to use the current context.")
            .scope(ConfigScope::Context),
        )
        .add_field(
            ConfigField::text(config::NAMESPACES_KEY, ConfigSection::Settings, "Namespaces")
                .description(
                    "Comma-separated namespaces to read workflow templates, cron workflows and workflows from. Leave empty to read every namespace, which needs cluster-wide list permission on workflowtemplates and cronworkflows.",
                )
                .validation(kubernetes::NAMESPACE_LIST_REGEX, kubernetes::NAMESPACE_LIST_MESSAGE)
                .scope(ConfigScope::Resources),
        )
        .add_field(status_page_field())
}
