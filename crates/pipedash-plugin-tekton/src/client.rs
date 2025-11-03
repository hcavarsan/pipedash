use crate::{config, types::{PipelineList, PipelineRunList, TektonPipeline, TektonPipelineRun}};
use kube::Client;
use pipedash_plugin_api::{PluginError, PluginResult};
use std::path::PathBuf;

pub struct TektonClient {
    client: Client,
}

impl TektonClient {
    fn merge_kubeconfigs(paths: Vec<String>) -> PluginResult<kube::config::Kubeconfig> {
        let mut merged = kube::config::Kubeconfig {
            preferences: None,
            clusters: vec![],
            auth_infos: vec![],
            contexts: vec![],
            current_context: None,
            extensions: None,
            kind: None,
            api_version: None,
        };

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
        kubeconfig_path: Option<&str>,
        context: Option<&str>,
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

        let config = kube::Config::from_custom_kubeconfig(kubeconfig, &options)
            .await
            .map_err(|e| {
                PluginError::InvalidConfig(format!("Failed to load kubeconfig: {}", e))
            })?;

        let client = Client::try_from(config).map_err(|e| {
            PluginError::InvalidConfig(format!("Failed to create Kubernetes client: {}", e))
        })?;

        Ok(Self { client })
    }

    pub async fn list_namespaces(&self) -> PluginResult<Vec<String>> {
        use kube::api::{Api, ListParams};

        let namespaces_api: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(self.client.clone());

        let namespaces = namespaces_api
            .list(&ListParams::default())
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to list namespaces: {}", e)))?;

        Ok(namespaces
            .items
            .into_iter()
            .filter_map(|ns| ns.metadata.name)
            .collect())
    }

    pub async fn list_namespaces_with_tekton(&self) -> PluginResult<Vec<String>> {
        let all_namespaces = self.list_namespaces().await?;
        let mut tekton_namespaces = Vec::new();

        for namespace in all_namespaces {
            if let Ok(pipelines) = self.list_pipelines(&namespace).await {
                if !pipelines.is_empty() || self.has_tekton_crds(&namespace).await {
                    tekton_namespaces.push(namespace);
                }
            }
        }

        Ok(tekton_namespaces)
    }

    async fn has_tekton_crds(&self, namespace: &str) -> bool {
        self.list_pipelines(namespace).await.is_ok()
    }

    async fn request_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> PluginResult<T> {
        let request = http::Request::builder()
            .uri(url)
            .method(http::Method::GET)
            .body(Vec::new())
            .map_err(|e| PluginError::Internal(format!("Failed to build request: {}", e)))?;

        let response_body = self
            .client
            .request_text(request)
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to make request: {}", e)))?;

        serde_json::from_str(&response_body)
            .map_err(|e| PluginError::SerializationError(format!("Failed to parse response: {}", e)))
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> PluginResult<T> {
        let body_bytes = serde_json::to_vec(body)
            .map_err(|e| PluginError::SerializationError(format!("Failed to serialize request: {}", e)))?;

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
            .map_err(|e| PluginError::ApiError(format!("Failed to make request: {}", e)))?;

        serde_json::from_str(&response_body)
            .map_err(|e| PluginError::SerializationError(format!("Failed to parse response: {}", e)))
    }

    pub async fn list_pipelines(&self, namespace: &str) -> PluginResult<Vec<TektonPipeline>> {
        let url = format!("/apis/tekton.dev/v1/namespaces/{}/pipelines", namespace);
        let pipeline_list: PipelineList = self.request_json(&url).await?;
        Ok(pipeline_list.items)
    }

    pub async fn get_pipeline(
        &self,
        namespace: &str,
        name: &str,
    ) -> PluginResult<TektonPipeline> {
        let url = format!("/apis/tekton.dev/v1/namespaces/{}/pipelines/{}", namespace, name);
        self.request_json(&url).await
    }

    pub async fn list_pipelineruns(
        &self,
        namespace: &str,
        pipeline_name: Option<&str>,
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
        &self,
        namespace: &str,
        pipelinerun: &TektonPipelineRun,
    ) -> PluginResult<TektonPipelineRun> {
        let url = format!("/apis/tekton.dev/v1/namespaces/{}/pipelineruns", namespace);
        let body = serde_json::to_value(pipelinerun)
            .map_err(|e| PluginError::SerializationError(format!("Failed to serialize pipelinerun: {}", e)))?;
        self.post_json(&url, &body).await
    }

    pub async fn delete_pipelinerun(&self, namespace: &str, name: &str) -> PluginResult<()> {
        let request = http::Request::builder()
            .uri(format!("/apis/tekton.dev/v1/namespaces/{}/pipelineruns/{}", namespace, name))
            .method(http::Method::DELETE)
            .body(Vec::new())
            .map_err(|e| PluginError::Internal(format!("Failed to build request: {}", e)))?;

        self.client
            .request_text(request)
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to delete pipelinerun: {}", e)))?;

        Ok(())
    }
}
