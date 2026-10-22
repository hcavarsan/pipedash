use pipedash_plugin_api::{
    ConfigField,
    ConfigFieldType,
    ConfigSchema,
    Feature,
    Permission,
    PluginCapabilities,
    PluginMetadata,
};

use crate::{
    config,
    schema,
    types::Kind,
};

const NAMESPACE_LIST_REGEX: &str =
    r"^$|^[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?(\s*,\s*[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?)*$";

pub(crate) struct PermissionSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub kind: Kind,
    pub verbs: &'static [&'static str],
}

impl PermissionSpec {
    pub fn permission(&self) -> Permission {
        Permission {
            name: self.name.to_string(),
            description: self.description.to_string(),
            required: self.required,
        }
    }
}

const READ_KUSTOMIZATIONS: &str = "Read Kustomizations";
const READ_HELMRELEASES: &str = "Read HelmReleases";
const RECONCILE_KUSTOMIZATIONS: &str = "Request Kustomization reconciliation";
const RECONCILE_HELMRELEASES: &str = "Request HelmRelease reconciliation";

pub(crate) const PERMISSIONS: [PermissionSpec; 4] = [
    PermissionSpec {
        name: READ_KUSTOMIZATIONS,
        description: "get and list kustomizations.kustomize.toolkit.fluxcd.io to show status, conditions and history",
        required: true,
        kind: Kind::Kustomization,
        verbs: &["get", "list"],
    },
    PermissionSpec {
        name: READ_HELMRELEASES,
        description: "get and list helmreleases.helm.toolkit.fluxcd.io to show status, conditions and release history",
        required: true,
        kind: Kind::HelmRelease,
        verbs: &["get", "list"],
    },
    PermissionSpec {
        name: RECONCILE_KUSTOMIZATIONS,
        description: "patch kustomizations.kustomize.toolkit.fluxcd.io to set the reconcile.fluxcd.io/requestedAt annotation",
        required: false,
        kind: Kind::Kustomization,
        verbs: &["patch"],
    },
    PermissionSpec {
        name: RECONCILE_HELMRELEASES,
        description: "patch helmreleases.helm.toolkit.fluxcd.io to set the reconcile.fluxcd.io/requestedAt annotation",
        required: false,
        kind: Kind::HelmRelease,
        verbs: &["patch"],
    },
];

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "FluxCD".to_string(),
        provider_type: config::PROVIDER_TYPE.to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor Flux Kustomizations and HelmReleases, inspect their status and history, and request reconciliation. Flux retains a limited history and groups repeated reconciliations.".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("/provider-icons/fluxcd.svg".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: PERMISSIONS.iter().map(PermissionSpec::permission).collect(),
        features: create_features(),
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(ConfigField {
            key: "kubeconfig".to_string(),
            label: "Kubeconfig".to_string(),
            description: Some(
                "Path on the machine running Pipedash, not in your browser. Defaults to $KUBECONFIG or ~/.kube/config. Separate files with ':' (';' on Windows). Monitoring needs get/list permissions. Reconciliation also needs patch permission."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: Some(serde_json::Value::String(config::default_kubeconfig())),
            options: None,
            validation_regex: None,
            validation_message: None,
        })
        .add_field(ConfigField {
            key: "context".to_string(),
            label: "Kubernetes context".to_string(),
            description: Some(
                "Context from the kubeconfig that points at the Flux cluster. Leave empty to use current-context."
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
            key: "namespaces".to_string(),
            label: "Namespaces".to_string(),
            description: Some(
                "Comma-separated namespaces to monitor, for example flux-system,apps. Leave empty to monitor Kustomizations and HelmReleases in every namespace, which needs cluster-wide list permission. List your namespaces when your RBAC is namespace-scoped."
                    .to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: None,
            options: None,
            validation_regex: Some(NAMESPACE_LIST_REGEX.to_string()),
            validation_message: Some(
                "Use comma-separated Kubernetes namespace names: lowercase letters, numbers and hyphens, starting and ending with a letter or number, up to 63 characters each."
                    .to_string(),
            ),
        })
}

fn create_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        pipelines: true,
        pipeline_runs: true,
        trigger: true,
        cancel: false,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
    }
}

fn create_features() -> Vec<Feature> {
    vec![
        Feature {
            id: "view_status".to_string(),
            name: "View status and history".to_string(),
            description:
                "Show Flux conditions and the reconciliation history kept in status.history"
                    .to_string(),
            required_permissions: vec![
                READ_KUSTOMIZATIONS.to_string(),
                READ_HELMRELEASES.to_string(),
            ],
        },
        Feature {
            id: "request_reconcile".to_string(),
            name: "Request reconciliation".to_string(),
            description:
                "Ask Flux to reconcile a resource now. Suspended resources are never resumed."
                    .to_string(),
            required_permissions: vec![
                RECONCILE_KUSTOMIZATIONS.to_string(),
                RECONCILE_HELMRELEASES.to_string(),
            ],
        },
    ]
}
