use std::collections::HashMap;

use futures::future::{
    join_all,
    try_join_all,
};
use k8s_openapi::api::apps::v1::Deployment;
use pipedash_plugin_api::health::deployment_health;
use pipedash_plugin_api::kubernetes::KubeconfigSource;
use pipedash_plugin_api::{
    collect_controller_health,
    ComponentHealth,
    PermissionCheck,
    PermissionStatus,
    PluginResult,
};
use tracing::warn;

use super::TektonPlugin;
use crate::{
    client,
    config,
    metadata,
};

const CONTROLLER_DEPLOYMENTS: [&str; 2] =
    ["tekton-pipelines-controller", "tekton-pipelines-webhook"];

pub(super) fn classify_controller(name: &str, deployment: &Deployment) -> Option<ComponentHealth> {
    let desired = deployment.spec.as_ref().and_then(|spec| spec.replicas);
    let status = deployment.status.as_ref();
    let available = status.and_then(|status| status.available_replicas);
    let unavailable = status
        .and_then(|status| status.conditions.as_ref())
        .is_some_and(|conditions| {
            conditions
                .iter()
                .any(|condition| condition.type_ == "Available" && condition.status == "False")
        });

    deployment_health(name, desired, available, unavailable)
}

impl TektonPlugin {
    pub(super) async fn check_health_impl(&self) -> PluginResult<Vec<ComponentHealth>> {
        let client = self.client().await?;
        let namespace = config::get_controller_namespace(&self.config);

        let checks = join_all(CONTROLLER_DEPLOYMENTS.iter().map(|name| async {
            client
                .get_deployment(&namespace, name)
                .await
                .map(|deployment| classify_controller(name, &deployment))
        }))
        .await;

        collect_controller_health(checks)
    }

    pub(super) async fn check_permissions_impl(&self) -> PluginResult<PermissionStatus> {
        let client = self.client().await?;
        let namespace = config::get_controller_namespace(&self.config);

        let allowed = try_join_all(
            CONTROLLER_DEPLOYMENTS
                .iter()
                .map(|name| client.can_i(&namespace, "apps", "deployments", "get", name)),
        )
        .await?;

        let permissions = vec![PermissionCheck {
            permission: metadata::controller_permission(),
            granted: allowed.into_iter().all(|allowed| allowed),
        }];
        let all_granted = permissions
            .iter()
            .filter(|check| check.permission.required)
            .all(|check| check.granted);

        Ok(PermissionStatus {
            permissions,
            all_granted,
            checked_at: chrono::Utc::now(),
            metadata: HashMap::new(),
            credential_label: None,
        })
    }

    pub(super) async fn get_field_options_impl(
        &self, field_key: &str, config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        match field_key {
            "context" => {
                client::config::available_contexts(KubeconfigSource::from_config(
                    config,
                    config::KUBECONFIG_PATH_KEY,
                ))
                .await
            }
            "namespaces" => Ok(namespace_suggestions(config).await),
            _ => Ok(Vec::new()),
        }
    }
}

async fn namespace_suggestions(config: &HashMap<String, String>) -> Vec<String> {
    let context = config::get_context(config);
    let client = match client::TektonClient::from_kubeconfig(
        &KubeconfigSource::from_config(config, config::KUBECONFIG_PATH_KEY),
        context.as_deref(),
    )
    .await
    {
        Ok(client) => client,
        Err(e) => {
            warn!(error = %e, "Failed to create Tekton client for namespace autocomplete");
            return Vec::new();
        }
    };

    client
        .try_list_namespaces_cluster_wide()
        .await
        .unwrap_or_else(|e| {
            warn!(error = %e, "Failed to fetch namespaces for Tekton autocomplete");
            Vec::new()
        })
}
