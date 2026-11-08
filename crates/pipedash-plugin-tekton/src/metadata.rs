use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

const NAMESPACE_LIST_REGEX: &str =
    r"^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)(\s*,\s*[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$";
const NAMESPACE_REGEX: &str = r"^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)?$";

pub(crate) const CONTROLLER_HEALTH_PERMISSION: &str = "Read controller Deployments";

pub(crate) fn controller_permission() -> Permission {
    Permission::new(
        CONTROLLER_HEALTH_PERMISSION,
        "get deployments.apps tekton-pipelines-controller and tekton-pipelines-webhook in the Tekton controller namespace to report controller health",
        false,
    )
}

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Tekton CD",
        create_config_schema(),
        schema::create_table_schema(),
        PluginCapabilities {
            uses_http_proxy: false,
            ..PluginCapabilities::standard()
        },
    )
    .with_permissions(vec![controller_permission()])
    .with_features(vec![Feature::new(
        CONTROLLER_HEALTH_FEATURE,
        "Controller health",
        "Warn when the Tekton controller or webhook is scaled to zero, not running or only partly available",
        &[CONTROLLER_HEALTH_PERMISSION],
    )])
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text(
                "kubeconfig_path",
                ConfigSection::Authentication,
                "Kubeconfig Path",
            )
            .description(
                "Path to your Kubernetes config file(s). Multiple paths can be separated by ':' (Unix) or ';' (Windows). Uses $KUBECONFIG env var if set. Ignored when Kubeconfig Content is set.",
            )
            .default_text(&config::get_default_kubeconfig_path()),
        )
        .add_field(ConfigField::kubeconfig_content(
            config::KUBECONFIG_CONTENT_KEY,
            "Kubeconfig Content",
        ))
        .add_field(
            ConfigField::select(
                "context",
                ConfigSection::Authentication,
                "Kubernetes Context",
                &[],
            )
            .description(
                "Select a context from your kubeconfig. Leave empty to use current-context.",
            ),
        )
        .add_field(
            ConfigField::select(
                "namespace_mode",
                ConfigSection::Settings,
                "Namespace Discovery Mode",
                &["all", "custom"],
            )
            .description(
                "How to discover namespaces containing Tekton pipelines:\n• 'all' - Automatically discover all namespaces (requires cluster-wide namespace list permissions)\n• 'custom' - Manually specify namespaces (recommended for users without admin permissions or on OpenShift/RHOS)\n\nIf you see a '403 Forbidden' error during validation, switch to 'custom' mode and specify namespaces manually.",
            )
            .default_text("all"),
        )
        .add_field(
            ConfigField::text("namespaces", ConfigSection::Settings, "Namespaces")
                .description(
                    "Comma-separated list of namespaces to monitor (e.g., 'default,prod,staging'). Required when namespace mode is 'custom'. Leave empty when mode is 'all' to auto-discover. Each namespace must be a valid Kubernetes namespace name (lowercase alphanumeric, hyphens, max 63 chars). Run logs need 'get' permission on pods and pods/log in each monitored namespace.",
                )
                .validation(
                    NAMESPACE_LIST_REGEX,
                    "Invalid namespace format. Each namespace must: start and end with alphanumeric, contain only lowercase letters, numbers, and hyphens, be max 63 characters. Separate multiple namespaces with commas.",
                ),
        )
        .add_field(
            ConfigField::text(
                "tekton_namespace",
                ConfigSection::Settings,
                "Tekton controller namespace",
            )
            .description(
                "Namespace where the Tekton Pipelines controller and webhook run. Pipedash reads the tekton-pipelines-controller and tekton-pipelines-webhook Deployments there to report controller health. Defaults to tekton-pipelines. Needs 'get' permission on Deployments in that namespace; without it, controller health is unavailable.",
            )
            .default_text(config::DEFAULT_CONTROLLER_NAMESPACE)
            .validation(
                NAMESPACE_REGEX,
                "Must be a valid Kubernetes namespace name: lowercase letters, numbers and hyphens, up to 63 characters.",
            ),
        )
        .add_field(status_page_field())
}
