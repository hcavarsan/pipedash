use k8s_openapi::api::apps::v1::Deployment;
use kube::Client;
use pipedash_plugin_api::kubernetes::{
    self,
    map_error,
    KubeconfigSource,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    RetryPolicy,
};
use serde::Deserialize;

use self::config::load_kubeconfig;
use crate::types::{
    PipelineList,
    PipelineRunList,
    TaskRunList,
    TektonPipeline,
    TektonPipelineRun,
    TektonTaskRun,
};

pub(crate) mod config;
mod logs;
mod namespaces;

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

fn build_request(
    builder: http::request::Builder, body: Vec<u8>,
) -> PluginResult<http::Request<Vec<u8>>> {
    builder
        .body(body)
        .map_err(|e| PluginError::Internal(format!("Failed to build request: {}", e)))
}

pub(crate) const STEP_LOG_TAIL_LINES: usize = 10_000;

impl TektonClient {
    pub async fn from_kubeconfig(
        source: &KubeconfigSource, context: Option<&str>,
    ) -> PluginResult<Self> {
        let source = source.clone();
        let client = kubernetes::connect(
            move || load_kubeconfig(&source),
            context.map(str::to_string),
            true,
        )
        .await?;

        Ok(Self {
            client,
            retry_policy: RetryPolicy::default(),
        })
    }

    async fn request_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> PluginResult<T> {
        let url = url.to_string();

        self.retry_policy
            .retry(|| async {
                let request = build_request(
                    http::Request::builder().uri(&url).method(http::Method::GET),
                    Vec::new(),
                )?;
                self.send_json(request).await
            })
            .await
    }

    async fn send_json<T: serde::de::DeserializeOwned>(
        &self, request: http::Request<Vec<u8>>,
    ) -> PluginResult<T> {
        let response_body = self
            .client
            .request_text(request)
            .await
            .map_err(|e| map_error(e, "Failed to make request"))?;

        serde_json::from_str(&response_body).map_err(|e| {
            PluginError::SerializationError(format!("Failed to parse response: {}", e))
        })
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self, url: &str, body: &serde_json::Value,
    ) -> PluginResult<T> {
        let body_bytes = serde_json::to_vec(body).map_err(|e| {
            PluginError::SerializationError(format!("Failed to serialize request: {}", e))
        })?;

        let request = build_request(
            http::Request::builder()
                .uri(url)
                .method(http::Method::POST)
                .header("Content-Type", "application/json"),
            body_bytes,
        )?;
        self.send_json(request).await
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

    pub async fn get_pipelinerun(
        &self, namespace: &str, name: &str,
    ) -> PluginResult<TektonPipelineRun> {
        self.request_json(&format!(
            "/apis/tekton.dev/v1/namespaces/{namespace}/pipelineruns/{name}"
        ))
        .await
    }

    pub async fn get_taskrun(&self, namespace: &str, name: &str) -> PluginResult<TektonTaskRun> {
        self.request_json(&format!(
            "/apis/tekton.dev/v1/namespaces/{namespace}/taskruns/{name}"
        ))
        .await
    }

    pub async fn list_taskruns(
        &self, namespace: &str, pipelinerun_name: &str,
    ) -> PluginResult<Vec<TektonTaskRun>> {
        let list: TaskRunList = self
            .request_json(&format!(
                "/apis/tekton.dev/v1/namespaces/{namespace}/taskruns?labelSelector=tekton.dev/pipelineRun={pipelinerun_name}"
            ))
            .await?;
        Ok(list.items)
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
        let request = build_request(
            http::Request::builder()
                .uri(format!(
                    "/apis/tekton.dev/v1/namespaces/{}/pipelineruns/{}",
                    namespace, name
                ))
                .method(http::Method::DELETE),
            Vec::new(),
        )?;

        self.client
            .request_text(request)
            .await
            .map_err(|e| map_error(e, "Failed to delete pipelinerun"))?;

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
