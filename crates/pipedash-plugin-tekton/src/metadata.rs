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
    Permission {
        name: CONTROLLER_HEALTH_PERMISSION.to_string(),
        description: "get deployments.apps tekton-pipelines-controller and tekton-pipelines-webhook in the Tekton controller namespace to report controller health".to_string(),
        required: false,
    }
}

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "Tekton CD".to_string(),
        provider_type: "tekton".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger Tekton CI/CD pipelines running on Kubernetes, using a kubeconfig file or pasted kubeconfig content".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/tekton.svg".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: vec![controller_permission()],
        features: vec![Feature {
            id: CONTROLLER_HEALTH_FEATURE.to_string(),
            name: "Controller health".to_string(),
            description: "Warn when the Tekton controller or webhook is scaled to zero, not running or only partly available".to_string(),
            required_permissions: vec![CONTROLLER_HEALTH_PERMISSION.to_string()],
        }],
    }
}

fn create_config_schema() -> ConfigSchema {
    let default_kubeconfig = config::get_default_kubeconfig_path();

    ConfigSchema::new()
        .add_field(ConfigField {
            key: "kubeconfig_path".to_string(),
            label: "Kubeconfig Path".to_string(),
            description: Some(
                "Path to your Kubernetes config file(s). Multiple paths can be separated by ':' (Unix) or ';' (Windows). Uses $KUBECONFIG env var if set. Ignored when Kubeconfig Content is set."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: Some(serde_json::Value::String(default_kubeconfig)),
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: config::KUBECONFIG_CONTENT_KEY.to_string(),
            label: "Kubeconfig Content".to_string(),
            description: Some(
                "Paste the kubeconfig YAML instead of using a file path, for example on iOS and Android where no kubeconfig file exists. When set, it is used instead of the path above and is stored with your other credentials. Use token or client certificate authentication: exec credential plugins (aws, gke-gcloud-auth-plugin, kubelogin) are not supported on iOS and Android."
                    .to_string(),
            ),
            field_type: ConfigFieldType::TextArea,
            required: false,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "context".to_string(),
            label: "Kubernetes Context".to_string(),
            description: Some(
                "Select a context from your kubeconfig. Leave empty to use current-context."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Select,
            required: false,
            default_value: None,
            options: Some(Vec::new()),
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "namespace_mode".to_string(),
            label: "Namespace Discovery Mode".to_string(),
            description: Some(
                "How to discover namespaces containing Tekton pipelines:\n• 'all' - Automatically discover all namespaces (requires cluster-wide namespace list permissions)\n• 'custom' - Manually specify namespaces (recommended for users without admin permissions or on OpenShift/RHOS)\n\nIf you see a '403 Forbidden' error during validation, switch to 'custom' mode and specify namespaces manually."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Select,
            required: false,
            default_value: Some(serde_json::Value::String("all".to_string())),
            options: Some(vec![
                "all".to_string(),
                "custom".to_string(),
            ]),
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "namespaces".to_string(),
            label: "Namespaces".to_string(),
            description: Some(
                "Comma-separated list of namespaces to monitor (e.g., 'default,prod,staging'). Required when namespace mode is 'custom'. Leave empty when mode is 'all' to auto-discover. Each namespace must be a valid Kubernetes namespace name (lowercase alphanumeric, hyphens, max 63 chars)."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: None,
            options: None,
            validation_regex: Some(NAMESPACE_LIST_REGEX.to_string()),
            validation_message: Some(
                "Invalid namespace format. Each namespace must: start and end with alphanumeric, contain only lowercase letters, numbers, and hyphens, be max 63 characters. Separate multiple namespaces with commas.".to_string(),
            ),
        })
        .add_field(ConfigField {
            key: "tekton_namespace".to_string(),
            label: "Tekton controller namespace".to_string(),
            description: Some(
                "Namespace where the Tekton Pipelines controller and webhook run. Pipedash reads the tekton-pipelines-controller and tekton-pipelines-webhook Deployments there to report controller health. Defaults to tekton-pipelines. Needs 'get' permission on Deployments in that namespace; without it, controller health is unavailable."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: Some(serde_json::Value::String(
                config::DEFAULT_CONTROLLER_NAMESPACE.to_string(),
            )),
            options: None,
            validation_regex: Some(NAMESPACE_REGEX.to_string()),
            validation_message: Some(
                "Must be a valid Kubernetes namespace name: lowercase letters, numbers and hyphens, up to 63 characters.".to_string(),
            ),
        })
        .add_field(status_page_field())
}

fn create_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        pipelines: true,
        pipeline_runs: true,
        trigger: true,
        cancel: true,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
        retains_run_history: false,
    }
}
