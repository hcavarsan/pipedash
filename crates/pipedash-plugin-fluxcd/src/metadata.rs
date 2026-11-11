use pipedash_plugin_api::kubernetes;
use pipedash_plugin_api::{
    status_page_field,
    ConfigField,
    ConfigSchema,
    ConfigSection,
    Feature,
    Permission,
    PluginCapabilities,
    PluginMetadata,
    RunHistoryOrder,
    RunHistoryPolicy,
    CONTROLLER_HEALTH_FEATURE,
};

use crate::{
    config,
    presentation,
    schema,
    types::Kind,
};

const FLUX_NAMESPACE_REGEX: &str = r"^$|^[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?$";

pub(crate) struct PermissionSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub group: &'static str,
    pub resources: &'static [&'static str],
    pub verbs: &'static [&'static str],
    pub names: &'static [&'static str],
    pub cluster_wide: bool,
}

impl PermissionSpec {
    pub fn permission(&self) -> Permission {
        Permission::new(self.name, self.description, self.required)
    }
}

const READ_KUSTOMIZATIONS: &str = "Read Kustomizations";
const READ_HELMRELEASES: &str = "Read HelmReleases";
const RECONCILE_KUSTOMIZATIONS: &str = "Request Kustomization reconciliation";
const RECONCILE_HELMRELEASES: &str = "Request HelmRelease reconciliation";
const READ_CONTROLLERS: &str = "Read Flux controller deployments";
const READ_EVENTS: &str = "Read Flux object events";
const READ_WORKLOADS: &str = "Read managed workloads";

pub(crate) const PERMISSIONS: [PermissionSpec; 7] = [
    PermissionSpec {
        name: READ_KUSTOMIZATIONS,
        description: "get and list kustomizations.kustomize.toolkit.fluxcd.io to show status, conditions and history",
        required: true,
        group: Kind::Kustomization.group(),
        resources: &[Kind::Kustomization.plural()],
        verbs: &["get", "list"],
        names: &[],
        cluster_wide: false,
    },
    PermissionSpec {
        name: READ_HELMRELEASES,
        description: "get and list helmreleases.helm.toolkit.fluxcd.io to show status, conditions and release history",
        required: true,
        group: Kind::HelmRelease.group(),
        resources: &[Kind::HelmRelease.plural()],
        verbs: &["get", "list"],
        names: &[],
        cluster_wide: false,
    },
    PermissionSpec {
        name: RECONCILE_KUSTOMIZATIONS,
        description: "patch kustomizations.kustomize.toolkit.fluxcd.io to set the reconcile.fluxcd.io/requestedAt annotation",
        required: false,
        group: Kind::Kustomization.group(),
        resources: &[Kind::Kustomization.plural()],
        verbs: &["patch"],
        names: &[],
        cluster_wide: false,
    },
    PermissionSpec {
        name: RECONCILE_HELMRELEASES,
        description: "patch helmreleases.helm.toolkit.fluxcd.io to set the reconcile.fluxcd.io/requestedAt annotation",
        required: false,
        group: Kind::HelmRelease.group(),
        resources: &[Kind::HelmRelease.plural()],
        verbs: &["patch"],
        names: &[],
        cluster_wide: false,
    },
    PermissionSpec {
        name: READ_CONTROLLERS,
        description: "get deployments.apps named source-controller, kustomize-controller and helm-controller in the Flux namespace to show controller health",
        required: false,
        group: "apps",
        resources: &["deployments"],
        verbs: &["get"],
        names: &config::CONTROLLERS,
        cluster_wide: false,
    },
    PermissionSpec {
        name: READ_EVENTS,
        description: "list events in the namespace of each monitored resource to show the Kubernetes events of a Kustomization or HelmRelease",
        required: false,
        group: "",
        resources: &["events"],
        verbs: &["list"],
        names: &[],
        cluster_wide: false,
    },
    PermissionSpec {
        name: READ_WORKLOADS,
        description: "list deployments, statefulsets and daemonsets in apps across all namespaces to report a Ready Flux resource as Degraded when the workloads it manages are unhealthy",
        required: false,
        group: "apps",
        resources: &["deployments", "statefulsets", "daemonsets"],
        verbs: &["list"],
        names: &[],
        cluster_wide: true,
    },
];

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        presentation: presentation::presentation(),
        run_history: RunHistoryPolicy {
            bounded: true,
            order: RunHistoryOrder::StartedAt,
            tracks_overdue: false,
        },
        ..PluginMetadata::new(
            "FluxCD",
            create_config_schema(),
            schema::create_table_schema(),
            PluginCapabilities {
                cancel: false,
                retains_run_history: true,
                uses_http_proxy: false,
                async_trigger: true,
                ..PluginCapabilities::standard()
            },
        )
        .with_permissions(PERMISSIONS.iter().map(PermissionSpec::permission).collect())
        .with_features(create_features())
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(
            ConfigField::text(config::KUBECONFIG_PATH_KEY, ConfigSection::Authentication, "Kubeconfig")
                .description(
                    "Path on the machine running Pipedash, not in your browser. Defaults to $KUBECONFIG or ~/.kube/config. Separate files with ':' (';' on Windows). Ignored when Kubeconfig content is set. Monitoring needs get/list permissions. Reconciliation also needs patch permission. Run logs also need list on events in each monitored namespace.",
                )
                .default_text(&kubernetes::default_kubeconfig_path()),
        )
        .add_field(ConfigField::kubeconfig_content(
            kubernetes::KUBECONFIG_CONTENT_KEY,
            "Kubeconfig content",
        ))
        .add_field(
            ConfigField::select(
                "context",
                ConfigSection::Authentication,
                "Kubernetes context",
                &[],
            )
            .description(
                "Context from the kubeconfig that points at the Flux cluster. Leave empty to use current-context.",
            ),
        )
        .add_field(
            ConfigField::text("namespaces", ConfigSection::Settings, "Namespaces")
                .description(
                    "Comma-separated namespaces to monitor, for example flux-system,apps. Leave empty to monitor Kustomizations and HelmReleases in every namespace, which needs cluster-wide list permission. List your namespaces when your RBAC is namespace-scoped.",
                )
                .validation(kubernetes::NAMESPACE_LIST_REGEX, kubernetes::NAMESPACE_LIST_MESSAGE),
        )
        .add_field(
            ConfigField::text("flux_namespace", ConfigSection::Settings, "Flux namespace")
                .description(
                    "Namespace where the Flux controllers run. Pipedash reads the source-controller, kustomize-controller and helm-controller deployments there to report controller health. Leave empty to use flux-system.",
                )
                .default_text(config::DEFAULT_FLUX_NAMESPACE)
                .validation(
                    FLUX_NAMESPACE_REGEX,
                    "Use a Kubernetes namespace name: lowercase letters, numbers and hyphens, starting and ending with a letter or number, up to 63 characters.",
                ),
        )
        .add_field(status_page_field())
}

fn create_features() -> Vec<Feature> {
    vec![
        Feature::new(
            "view_status",
            "View status and history",
            "Show Flux conditions and the reconciliation history kept in status.history",
            &[READ_KUSTOMIZATIONS, READ_HELMRELEASES],
        ),
        Feature::new(
            "request_reconcile",
            "Request reconciliation",
            "Ask Flux to reconcile a resource now. Suspended resources are never resumed.",
            &[RECONCILE_KUSTOMIZATIONS, RECONCILE_HELMRELEASES],
        ),
        Feature::new(
            CONTROLLER_HEALTH_FEATURE,
            "Controller health",
            "Report when the Flux controllers are scaled to zero, not running or only partly available",
            &[READ_CONTROLLERS],
        ),
        Feature::new(
            "run_logs",
            "Run logs",
            "Show the reconciliation record of a run and the Kubernetes events of the resource",
            &[READ_KUSTOMIZATIONS, READ_HELMRELEASES, READ_EVENTS],
        ),
    ]
}
