use futures::stream::{
    self,
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::kubernetes::map_error;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

use super::TektonClient;

const NAMESPACE_CONCURRENCY: usize = 8;

impl TektonClient {
    pub async fn list_namespaces(&self) -> PluginResult<Vec<String>> {
        self.retry_policy
            .retry(|| async {
                use kube::api::{
                    Api,
                    ListParams,
                };

                let namespaces_api: Api<k8s_openapi::api::core::v1::Namespace> =
                    Api::all(self.client.clone());

                let namespaces = namespaces_api
                    .list(&ListParams::default())
                    .await
                    .map_err(|e| map_error(e, "Failed to list namespaces"))?;

                Ok(namespaces
                    .items
                    .into_iter()
                    .filter_map(|ns| ns.metadata.name)
                    .collect())
            })
            .await
    }

    pub async fn try_list_namespaces_cluster_wide(&self) -> PluginResult<Vec<String>> {
        use kube::api::{
            Api,
            ListParams,
        };

        let namespaces_api: Api<k8s_openapi::api::core::v1::Namespace> =
            Api::all(self.client.clone());

        match namespaces_api.list(&ListParams::default()).await {
            Ok(namespaces) => Ok(namespaces
                .items
                .into_iter()
                .filter_map(|ns| ns.metadata.name)
                .collect()),
            Err(kube::Error::Api(status)) if status.code == 403 => {
                Err(PluginError::PermissionDenied(
                    "Missing cluster-wide namespace permissions. Please use 'custom' mode and specify namespaces manually in the configuration.".to_string()
                ))
            }
            Err(e) => Err(map_error(e, "Failed to list namespaces")),
        }
    }

    async fn filter_namespaces_with_pipelines(
        &self, namespaces: &[String],
    ) -> PluginResult<Vec<String>> {
        let checked: Vec<Option<String>> = stream::iter(namespaces.iter().cloned())
            .map(|namespace| async move {
                match self.list_pipelines(&namespace).await {
                    Ok(pipelines) => Ok(
                        (!pipelines.is_empty() || self.has_pipelines(&namespace).await)
                            .then(|| namespace.clone()),
                    ),
                    Err(e) if e.is_transient() => Err(e),
                    Err(e) => {
                        tracing::warn!(namespace = %namespace, error = %e, "Skipping namespace whose pipelines cannot be listed");
                        Ok(None)
                    }
                }
            })
            .buffered(NAMESPACE_CONCURRENCY)
            .try_collect()
            .await?;

        Ok(checked.into_iter().flatten().collect())
    }

    pub async fn list_namespaces_with_pipelines(&self) -> PluginResult<Vec<String>> {
        let all_namespaces = self.list_namespaces().await?;
        self.filter_namespaces_with_pipelines(&all_namespaces).await
    }

    pub async fn validate_namespaces_have_pipelines(
        &self, namespaces: &[String],
    ) -> PluginResult<Vec<String>> {
        if namespaces.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No namespaces specified. Please provide at least one namespace in the 'namespaces' field.".to_string()
            ));
        }

        let results: Vec<_> = stream::iter(namespaces.iter().cloned())
            .map(|namespace| async move {
                match self.list_pipelines(&namespace).await {
                    Ok(pipelines) if pipelines.is_empty() => Ok(None),
                    Ok(_) => Ok(Some(namespace.clone())),
                    Err(e) => Err(format!("Failed to access namespace '{}': {}", namespace, e)),
                }
            })
            .buffered(NAMESPACE_CONCURRENCY)
            .collect()
            .await;

        let mut valid_namespaces = Vec::new();
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(Some(ns)) => valid_namespaces.push(ns),
                Ok(None) => {}
                Err(e) => errors.push(e),
            }
        }

        if valid_namespaces.is_empty() {
            if errors.is_empty() {
                return Err(PluginError::InvalidConfig(
                    format!("No Tekton pipelines found in any of the specified namespaces: {:?}. Verify that Tekton is installed and pipelines exist in these namespaces.", namespaces)
                ));
            } else {
                return Err(PluginError::InvalidConfig(format!(
                    "Failed to validate namespaces. Errors: {}",
                    errors.join("; ")
                )));
            }
        }

        Ok(valid_namespaces)
    }

    async fn has_pipelines(&self, namespace: &str) -> bool {
        self.list_pipelines(namespace).await.is_ok()
    }
}
