use pipedash_plugin_api::kubernetes;
use pipedash_plugin_api::*;

use crate::{
    config,
    schema,
};

const API: &[AuthMode] = &[AuthMode::Token, AuthMode::SignIn];
const KUBE: &[AuthMode] = &[AuthMode::Kubeconfig];

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
        .auth_option(AuthOption::token(
            "API token",
            "Connects to the ArgoCD server with an account or project token.",
        ))
        .auth_option(AuthOption::sign_in(
            "Signs you in through the single sign-on set up on your ArgoCD server.",
        ))
        .auth_option(AuthOption::kubeconfig(
            "Reads Applications straight from the cluster with your kubeconfig. No ArgoCD server address or token is needed. ArgoCD project rules and sync windows still apply.",
        ))
        .add_auth_field(
            API,
            ConfigField::text("server_url", ConfigSection::Connection, "ArgoCD Server URL")
                .description("ArgoCD API server URL (e.g., https://argocd.example.com)")
                .required()
                .http_url_validation(),
        )
        .add_auth_field(API, ConfigField::insecure_tls())
        .add_auth_field(
            &[AuthMode::Token],
            ConfigField::token(
                "Authentication Token",
                "ArgoCD authentication token or Kubernetes service account token",
            ),
        )
        .add_auth_field(
            KUBE,
            ConfigField::text(
                config::KUBECONFIG_PATH_KEY,
                ConfigSection::Authentication,
                "Kubeconfig",
            )
            .description(
                "Path on the machine running Pipedash, not in your browser. Defaults to $KUBECONFIG or ~/.kube/config. Separate files with ':' (';' on Windows). Ignored when Kubeconfig content is set. Needs get and list on applications.argoproj.io. Syncing and cancelling also need patch. Pod health, pod images and pod logs need list on pods, replicasets and jobs. Pod logs also need get on pods/log and list on events. Identifying the cluster needs get on the kube-system namespace.",
            )
            .default_text(&kubernetes::default_kubeconfig_path()),
        )
        .add_auth_field(
            KUBE,
            ConfigField::kubeconfig_content(
                kubernetes::KUBECONFIG_CONTENT_KEY,
                "Kubeconfig content",
            ),
        )
        .add_auth_field(
            KUBE,
            ConfigField::select(
                "context",
                ConfigSection::Authentication,
                "Kubernetes context",
                &[],
            )
            .description(
                "Context that points at the cluster where ArgoCD runs. Leave empty to use current-context.",
            ),
        )
        .add_auth_field(
            KUBE,
            ConfigField::text(
                "namespaces",
                ConfigSection::Connection,
                "Application namespaces",
            )
            .description(
                "Comma-separated namespaces that hold ArgoCD Applications. Leave empty to read every namespace, which needs cluster-wide list permission.",
            )
            .default_text("argocd")
            .validation(
                kubernetes::NAMESPACE_LIST_REGEX,
                kubernetes::NAMESPACE_LIST_MESSAGE,
            ),
        )
        .add_field(status_page_field())
}
