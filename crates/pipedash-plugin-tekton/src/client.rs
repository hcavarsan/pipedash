use std::path::PathBuf;
use std::time::Duration;

use k8s_openapi::api::apps::v1::Deployment;
use kube::Client;
use pipedash_plugin_api::{
    from_status,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use serde::Deserialize;

use crate::{
    config,
    types::{
        PipelineList,
        PipelineRunList,
        TektonPipeline,
        TektonPipelineRun,
    },
};

pub struct TektonClient {
    client: Client,
    retry_policy: RetryPolicy,
}

#[derive(Deserialize)]
struct AccessReview {
    status: AccessStatus,
}

#[derive(Deserialize)]
struct AccessStatus {
    allowed: bool,
}

const MAX_RETRY_AFTER_SECONDS: u32 = 3600;

pub(crate) fn map_kube_error(error: kube::Error, context: &str) -> PluginError {
    match error {
        kube::Error::Api(status) => {
            let retry_after = status
                .details
                .as_ref()
                .map(|details| details.retry_after_seconds.min(MAX_RETRY_AFTER_SECONDS))
                .filter(|seconds| *seconds > 0)
                .map(|seconds| Duration::from_secs(seconds.into()));
            from_status(
                status.code,
                format!("{context}: {} (HTTP {})", status.message, status.code),
                retry_after,
            )
        }
        kube::Error::Auth(error) => {
            PluginError::AuthenticationFailed(format!("{context}: {error}"))
        }
        kube::Error::SerdeError(error) => {
            PluginError::SerializationError(format!("{context}: {error}"))
        }
        other => PluginError::NetworkError(format!("{context}: {other}")),
    }
}

impl TektonClient {
    fn merge_kubeconfigs(paths: Vec<String>) -> PluginResult<kube::config::Kubeconfig> {
        let mut merged = kube::config::Kubeconfig::default();

        let mut found_any = false;

        for path_str in paths {
            let path = PathBuf::from(&path_str);
            if !path.exists() {
                continue;
            }

            match kube::config::Kubeconfig::read_from(&path) {
                Ok(kc) => {
                    found_any = true;
                    merged.clusters.extend(kc.clusters);
                    merged.auth_infos.extend(kc.auth_infos);
                    merged.contexts.extend(kc.contexts);
                    if merged.current_context.is_none() {
                        merged.current_context = kc.current_context;
                    }
                }
                Err(_) => continue,
            }
        }

        if !found_any {
            return Err(PluginError::InvalidConfig(
                "No valid kubeconfig files found".to_string(),
            ));
        }

        Ok(merged)
    }

    pub async fn from_kubeconfig(
        kubeconfig_path: Option<&str>, context: Option<&str>,
    ) -> PluginResult<Self> {
        let kubeconfig = if let Some(path_str) = kubeconfig_path {
            let paths = config::split_kubeconfig_paths(path_str);
            Self::merge_kubeconfigs(paths)?
        } else {
            let default_path = config::get_default_kubeconfig_path();
            let paths = config::split_kubeconfig_paths(&default_path);
            Self::merge_kubeconfigs(paths)?
        };

        let options = if let Some(ctx) = context {
            kube::config::KubeConfigOptions {
                context: Some(ctx.to_string()),
                ..Default::default()
            }
        } else {
            kube::config::KubeConfigOptions::default()
        };

        let mut config = kube::Config::from_custom_kubeconfig(kubeconfig, &options)
            .await
            .map_err(|e| PluginError::InvalidConfig(format!("Failed to load kubeconfig: {}", e)))?;
        config.default_retry = false;

        let client = Client::try_from(config).map_err(|e| {
            PluginError::InvalidConfig(format!("Failed to create Kubernetes client: {}", e))
        })?;

        Ok(Self {
            client,
            retry_policy: RetryPolicy::default(),
        })
    }

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
                    .map_err(|e| map_kube_error(e, "Failed to list namespaces"))?;

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
            Err(e) => Err(map_kube_error(e, "Failed to list namespaces")),
        }
    }

    async fn filter_namespaces_with_pipelines(
        &self, namespaces: &[String],
    ) -> PluginResult<Vec<String>> {
        use futures::future::try_join_all;

        let check_futures = namespaces.iter().map(|namespace| async move {
            match self.list_pipelines(namespace).await {
                Ok(pipelines) => Ok(
                    (!pipelines.is_empty() || self.has_pipelines(namespace).await)
                        .then(|| namespace.clone()),
                ),
                Err(e) if e.is_transient() => Err(e),
                Err(e) => {
                    tracing::warn!(namespace = %namespace, error = %e, "Skipping namespace whose pipelines cannot be listed");
                    Ok(None)
                }
            }
        });

        Ok(try_join_all(check_futures)
            .await?
            .into_iter()
            .flatten()
            .collect())
    }

    pub async fn list_namespaces_with_pipelines(&self) -> PluginResult<Vec<String>> {
        let all_namespaces = self.list_namespaces().await?;
        self.filter_namespaces_with_pipelines(&all_namespaces).await
    }

    pub async fn validate_namespaces_have_pipelines(
        &self, namespaces: &[String],
    ) -> PluginResult<Vec<String>> {
        use futures::future::join_all;

        if namespaces.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No namespaces specified. Please provide at least one namespace in the 'namespaces' field.".to_string()
            ));
        }

        let validation_futures = namespaces.iter().map(|namespace| {
            let ns = namespace.clone();
            async move {
                match self.list_pipelines(&ns).await {
                    Ok(pipelines) => {
                        if !pipelines.is_empty() {
                            Ok(Some(ns))
                        } else {
                            Ok(None)
                        }
                    }
                    Err(e) => Err(format!("Failed to access namespace '{}': {}", ns, e)),
                }
            }
        });

        let results = join_all(validation_futures).await;

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

    async fn request_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> PluginResult<T> {
        let url = url.to_string();

        self.retry_policy
            .retry(|| async {
                let request = http::Request::builder()
                    .uri(&url)
                    .method(http::Method::GET)
                    .body(Vec::new())
                    .map_err(|e| {
                        PluginError::Internal(format!("Failed to build request: {}", e))
                    })?;

                let response_body = self
                    .client
                    .request_text(request)
                    .await
                    .map_err(|e| map_kube_error(e, "Failed to make request"))?;

                serde_json::from_str(&response_body).map_err(|e| {
                    PluginError::SerializationError(format!("Failed to parse response: {}", e))
                })
            })
            .await
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self, url: &str, body: &serde_json::Value,
    ) -> PluginResult<T> {
        let body_bytes = serde_json::to_vec(body).map_err(|e| {
            PluginError::SerializationError(format!("Failed to serialize request: {}", e))
        })?;

        let request = http::Request::builder()
            .uri(url)
            .method(http::Method::POST)
            .header("Content-Type", "application/json")
            .body(body_bytes)
            .map_err(|e| PluginError::Internal(format!("Failed to build request: {}", e)))?;

        let response_body = self
            .client
            .request_text(request)
            .await
            .map_err(|e| map_kube_error(e, "Failed to make request"))?;

        serde_json::from_str(&response_body).map_err(|e| {
            PluginError::SerializationError(format!("Failed to parse response: {}", e))
        })
    }

    pub async fn list_pipelines(&self, namespace: &str) -> PluginResult<Vec<TektonPipeline>> {
        let url = format!("/apis/tekton.dev/v1/namespaces/{}/pipelines", namespace);
        let pipeline_list: PipelineList = self.request_json(&url).await?;
        Ok(pipeline_list.items)
    }

    pub async fn get_pipeline(&self, namespace: &str, name: &str) -> PluginResult<TektonPipeline> {
        let url = format!(
            "/apis/tekton.dev/v1/namespaces/{}/pipelines/{}",
            namespace, name
        );
        self.request_json(&url).await
    }

    pub async fn list_pipelineruns(
        &self, namespace: &str, pipeline_name: Option<&str>,
    ) -> PluginResult<Vec<TektonPipelineRun>> {
        let url = if let Some(pipeline) = pipeline_name {
            format!(
                "/apis/tekton.dev/v1/namespaces/{}/pipelineruns?labelSelector=tekton.dev/pipeline={}",
                namespace, pipeline
            )
        } else {
            format!("/apis/tekton.dev/v1/namespaces/{}/pipelineruns", namespace)
        };

        let pipelinerun_list: PipelineRunList = self.request_json(&url).await?;
        Ok(pipelinerun_list.items)
    }

    pub async fn create_pipelinerun(
        &self, namespace: &str, pipelinerun: &TektonPipelineRun,
    ) -> PluginResult<TektonPipelineRun> {
        let url = format!("/apis/tekton.dev/v1/namespaces/{}/pipelineruns", namespace);
        let body = serde_json::to_value(pipelinerun).map_err(|e| {
            PluginError::SerializationError(format!("Failed to serialize pipelinerun: {}", e))
        })?;
        self.post_json(&url, &body).await
    }

    pub async fn delete_pipelinerun(&self, namespace: &str, name: &str) -> PluginResult<()> {
        let request = http::Request::builder()
            .uri(format!(
                "/apis/tekton.dev/v1/namespaces/{}/pipelineruns/{}",
                namespace, name
            ))
            .method(http::Method::DELETE)
            .body(Vec::new())
            .map_err(|e| PluginError::Internal(format!("Failed to build request: {}", e)))?;

        self.client
            .request_text(request)
            .await
            .map_err(|e| map_kube_error(e, "Failed to delete pipelinerun"))?;

        Ok(())
    }

    pub async fn get_deployment(&self, namespace: &str, name: &str) -> PluginResult<Deployment> {
        self.request_json(&format!(
            "/apis/apps/v1/namespaces/{namespace}/deployments/{name}"
        ))
        .await
    }

    pub async fn can_i(
        &self, namespace: &str, group: &str, resource: &str, verb: &str, name: &str,
    ) -> PluginResult<bool> {
        let body = serde_json::json!({
            "apiVersion": "authorization.k8s.io/v1",
            "kind": "SelfSubjectAccessReview",
            "spec": {
                "resourceAttributes": {
                    "namespace": namespace,
                    "verb": verb,
                    "group": group,
                    "resource": resource,
                    "name": name,
                }
            }
        });
        let review: AccessReview = self
            .post_json(
                "/apis/authorization.k8s.io/v1/selfsubjectaccessreviews",
                &body,
            )
            .await?;

        Ok(review.status.allowed)
    }
}
