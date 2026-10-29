use std::path::PathBuf;
use std::time::Duration;

use futures::AsyncBufReadExt;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{
    Api,
    LogParams,
};
use kube::Client;
use pipedash_plugin_api::{
    from_status,
    LogSink,
    PluginError,
    PluginResult,
    RetryPolicy,
    LOG_CHUNK_STALL_TIMEOUT,
};
use serde::Deserialize;

use crate::{
    config::{
        self,
        KubeconfigSource,
    },
    log_dialect,
    types::{
        PipelineList,
        PipelineRunList,
        TaskRunList,
        TektonPipeline,
        TektonPipelineRun,
        TektonTaskRun,
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
pub(crate) const STEP_LOG_TAIL_LINES: usize = 10_000;
const STEP_LOG_TIMEOUT: Duration = Duration::from_secs(900);

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

pub(crate) fn parse_kubeconfig_content(content: &str) -> PluginResult<kube::config::Kubeconfig> {
    kube::config::Kubeconfig::from_yaml(content).map_err(|_| {
        PluginError::InvalidConfig(
            "The pasted kubeconfig content is not valid kubeconfig YAML".to_string(),
        )
    })
}

const MOBILE_PLATFORM: bool = cfg!(any(target_os = "ios", target_os = "android"));

fn ensure_supported_auth(
    kubeconfig: &kube::config::Kubeconfig, context: Option<&str>, mobile: bool,
) -> PluginResult<()> {
    if !mobile {
        return Ok(());
    }

    let uses_exec = context
        .or(kubeconfig.current_context.as_deref())
        .and_then(|name| {
            kubeconfig
                .contexts
                .iter()
                .find(|candidate| candidate.name == name)
        })
        .and_then(|named| named.context.as_ref()?.user.as_deref())
        .and_then(|user| {
            kubeconfig
                .auth_infos
                .iter()
                .find(|candidate| candidate.name == user)
        })
        .filter(|named| {
            named
                .auth_info
                .as_ref()
                .is_some_and(|auth_info| auth_info.exec.is_some())
        });

    match uses_exec {
        Some(named) => Err(PluginError::NotSupported(format!(
            "Kubernetes user '{}' authenticates through an exec credential plugin, which is not supported on this platform. Use a token or client certificate in the kubeconfig instead.",
            named.name
        ))),
        None => Ok(()),
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
        source: &KubeconfigSource, context: Option<&str>,
    ) -> PluginResult<Self> {
        let kubeconfig = match source {
            KubeconfigSource::Content(content) => parse_kubeconfig_content(content)?,
            KubeconfigSource::Path(path) => {
                let paths = match path {
                    Some(path_str) => config::split_kubeconfig_paths(path_str),
                    None => config::split_kubeconfig_paths(&config::get_default_kubeconfig_path()),
                };
                Self::merge_kubeconfigs(paths)?
            }
        };
        ensure_supported_auth(&kubeconfig, context, MOBILE_PLATFORM)?;

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

    pub async fn stream_step_log(
        &self, namespace: &str, pod: &str, container: &str, sink: &mut LogSink,
    ) -> PluginResult<usize> {
        tokio::time::timeout(
            STEP_LOG_TIMEOUT,
            self.pipe_step_log(namespace, pod, container, sink),
        )
        .await
        .map_err(|_| {
            PluginError::NetworkError("Pod log download timed out after 900 seconds".into())
        })?
    }

    async fn pipe_step_log(
        &self, namespace: &str, pod: &str, container: &str, sink: &mut LogSink,
    ) -> PluginResult<usize> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let params = LogParams {
            container: Some(container.to_string()),
            timestamps: true,
            tail_lines: Some(STEP_LOG_TAIL_LINES as i64),
            ..LogParams::default()
        };
        let stream = self
            .retry_policy
            .retry(|| async {
                pods.log_stream(pod, &params)
                    .await
                    .map_err(|error| match error {
                        kube::Error::Api(status) if status.code == 404 => {
                            PluginError::PipelineNotFound(format!(
                                "The pod {pod} of this TaskRun no longer exists"
                            ))
                        }
                        other => map_kube_error(other, "Failed to read pod logs"),
                    })
            })
            .await?;
        let mut stream = std::pin::pin!(stream);

        let mut lines = 0;
        let mut ends_with_newline = true;
        loop {
            let chunk = match tokio::time::timeout(LOG_CHUNK_STALL_TIMEOUT, stream.fill_buf()).await
            {
                Err(_) => {
                    return Err(PluginError::NetworkError(
                        "Log download stalled for 60 seconds".into(),
                    ))
                }
                Ok(Err(e)) => {
                    return Err(PluginError::NetworkError(format!(
                        "Failed to read pod logs: {e}"
                    )))
                }
                Ok(Ok([])) => break,
                Ok(Ok(chunk)) => chunk,
            };
            let read = chunk.len();
            let clean = log_dialect::without_line_tags(chunk).into_owned();
            stream.consume_unpin(read);

            lines += clean.iter().filter(|b| **b == b'\n').count();
            if let Some(last) = clean.last() {
                ends_with_newline = *last == b'\n';
            }
            sink.write(clean.into()).await?;
        }
        if !ends_with_newline {
            sink.write(vec![b'\n'].into()).await?;
            lines += 1;
        }
        Ok(lines)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn kubeconfig_yaml(user_auth: &str) -> String {
        format!(
            "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: https://127.0.0.1:6443\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\n- name: other\n  context:\n    cluster: c\n    user: plain\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n{user_auth}\n- name: plain\n  user:\n    token: t\n"
        )
    }

    const EXEC_AUTH: &str = "    exec:\n      apiVersion: client.authentication.k8s.io/v1beta1\n      command: aws\n      args: [eks, get-token]";

    fn parse(yaml: &str) -> kube::config::Kubeconfig {
        kube::config::Kubeconfig::from_yaml(yaml).unwrap()
    }

    #[test]
    fn exec_auth_is_rejected_on_mobile_for_the_selected_user() {
        let kubeconfig = parse(&kubeconfig_yaml(EXEC_AUTH));

        for context in [None, Some("ctx")] {
            let error = ensure_supported_auth(&kubeconfig, context, true).unwrap_err();
            assert!(
                matches!(&error, PluginError::NotSupported(message) if message.contains("exec credential plugin") && message.contains("not supported on this platform")),
                "{error:?}"
            );
        }
    }

    #[test]
    fn exec_auth_is_allowed_off_mobile_and_for_other_users() {
        let kubeconfig = parse(&kubeconfig_yaml(EXEC_AUTH));

        assert!(ensure_supported_auth(&kubeconfig, None, false).is_ok());
        assert!(ensure_supported_auth(&kubeconfig, Some("other"), true).is_ok());
        assert!(ensure_supported_auth(&kubeconfig, Some("missing"), true).is_ok());

        let token_only = parse(&kubeconfig_yaml("    token: t"));
        assert!(ensure_supported_auth(&token_only, None, true).is_ok());
    }

    #[test]
    fn pasted_content_parses_without_touching_the_filesystem() {
        let kubeconfig = parse_kubeconfig_content(&kubeconfig_yaml("    token: t")).unwrap();

        assert_eq!(kubeconfig.contexts.len(), 2);
    }

    #[test]
    fn invalid_pasted_content_is_rejected_without_echoing_it() {
        let error = parse_kubeconfig_content("users: {token: s3cr3t-value").unwrap_err();

        assert!(matches!(&error, PluginError::InvalidConfig(_)), "{error:?}");
        assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
    }
}
