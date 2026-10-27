use std::sync::Arc;
use std::time::Duration;

use pipedash_plugin_api::{
    check_response,
    map_send_error,
    LogSink,
    PluginError,
    PluginResult,
    RetryPolicy,
    LOG_CHUNK_STALL_TIMEOUT,
};
use reqwest::Url;
use serde::de::DeserializeOwned;
use tracing::debug;

use crate::logs::{
    PodLogDecoder,
    POD_LOG_TAIL_LINES,
};
use crate::types::{
    Application,
    ApplicationList,
    ApplicationTree,
    Event,
    EventList,
    Pod,
    ResourceManifest,
    SyncRequest,
    SyncStrategy,
    SyncStrategyApply,
    SyncStrategyHook,
};

pub struct PodLogRequest<'a> {
    pub namespace: &'a str,
    pub pod: &'a str,
    pub container: &'a str,
    pub previous: bool,
}

pub struct ArgocdClient {
    http_client: Arc<reqwest::Client>,
    api_url: String,
    auth_header: String,
    retry_policy: RetryPolicy,
}

impl ArgocdClient {
    pub fn new(http_client: Arc<reqwest::Client>, server_url: &str, token: &str) -> Self {
        Self {
            http_client,
            api_url: Self::build_api_url(server_url),
            auth_header: format!("Bearer {}", token),
            retry_policy: RetryPolicy::default(),
        }
    }

    fn build_api_url(server_url: &str) -> String {
        format!("{}/api/v1", server_url.trim_end_matches('/'))
    }

    fn endpoint(&self, path: &str) -> PluginResult<Url> {
        let raw = format!("{}{}", self.api_url, path);
        Url::parse(&raw).map_err(|e| PluginError::InvalidConfig(format!("Invalid URL: {}", e)))
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &Url) -> PluginResult<T> {
        self.retry_policy
            .retry(|| async {
                let response = self
                    .http_client
                    .get(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .send()
                    .await
                    .map_err(|e| map_send_error(e, url))?;

                check_response(response)
                    .await?
                    .json::<T>()
                    .await
                    .map_err(|e| {
                        PluginError::ApiError(format!(
                            "Failed to parse ArgoCD API response: {}",
                            e.without_url()
                        ))
                    })
            })
            .await
    }

    pub async fn list_applications(
        &self, projects_filter: Option<&Vec<String>>,
    ) -> PluginResult<Vec<Application>> {
        let url = self.endpoint("/applications")?;
        let app_list: ApplicationList = self.get_json(&url).await?;

        Ok(match projects_filter {
            Some(projects) => app_list
                .items
                .into_iter()
                .filter(|app| projects.contains(&app.spec.project))
                .collect(),
            None => app_list.items,
        })
    }

    pub async fn get_application(&self, app_name: &str) -> PluginResult<Application> {
        let url = self.endpoint(&format!("/applications/{}", app_name))?;
        self.get_json(&url).await
    }

    pub async fn resource_tree(&self, app_name: &str) -> PluginResult<ApplicationTree> {
        let url = self.endpoint(&format!("/applications/{app_name}/resource-tree"))?;
        self.get_json(&url).await
    }

    pub async fn application_events(&self, app_name: &str) -> PluginResult<Vec<Event>> {
        let url = self.endpoint(&format!("/applications/{app_name}/events"))?;
        Ok(self.get_json::<EventList>(&url).await?.items)
    }

    pub async fn pod_events(
        &self, app_name: &str, namespace: &str, pod: &str, uid: &str,
    ) -> PluginResult<Vec<Event>> {
        let mut url = self.endpoint(&format!("/applications/{app_name}/events"))?;
        url.query_pairs_mut()
            .append_pair("resourceNamespace", namespace)
            .append_pair("resourceName", pod)
            .append_pair("resourceUID", uid);
        Ok(self.get_json::<EventList>(&url).await?.items)
    }

    pub async fn pod(&self, app_name: &str, namespace: &str, pod: &str) -> PluginResult<Pod> {
        let mut url = self.endpoint(&format!("/applications/{app_name}/resource"))?;
        url.query_pairs_mut()
            .append_pair("namespace", namespace)
            .append_pair("resourceName", pod)
            .append_pair("version", "v1")
            .append_pair("kind", "Pod");
        let resource: ResourceManifest = self.get_json(&url).await?;
        serde_json::from_str(&resource.manifest).map_err(|e| {
            PluginError::ApiError(format!("Failed to parse pod {namespace}/{pod}: {e}"))
        })
    }

    pub async fn stream_pod_log(
        &self, app_name: &str, request: PodLogRequest<'_>, sink: &mut LogSink,
    ) -> PluginResult<usize> {
        let mut url = self.endpoint(&format!("/applications/{app_name}/logs"))?;
        url.query_pairs_mut()
            .append_pair("namespace", request.namespace)
            .append_pair("podName", request.pod)
            .append_pair("container", request.container)
            .append_pair("tailLines", POD_LOG_TAIL_LINES)
            .append_pair("follow", "false");
        if request.previous {
            url.query_pairs_mut().append_pair("previous", "true");
        }

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .get(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .timeout(Duration::from_secs(900))
                    .send()
            })
            .await?;
        let mut response = check_response(response).await?;

        let mut decoder = PodLogDecoder::default();
        let mut out = Vec::new();
        while !decoder.done {
            match tokio::time::timeout(LOG_CHUNK_STALL_TIMEOUT, response.chunk()).await {
                Err(_) => {
                    return Err(PluginError::NetworkError(
                        "Log download stalled for 60 seconds".into(),
                    ))
                }
                Ok(Err(e)) => return Err(map_send_error(e, &url)),
                Ok(Ok(None)) => break,
                Ok(Ok(Some(chunk))) => decoder.feed(&chunk, &mut out)?,
            }
            if !out.is_empty() {
                sink.write(std::mem::take(&mut out).into()).await?;
            }
        }
        decoder.finish(&mut out)?;
        sink.write(out.into()).await?;
        Ok(decoder.lines)
    }

    pub async fn sync_application(
        &self, app_name: &str, revision: Option<String>, prune: bool, dry_run: bool, force: bool,
        apply_only: bool,
    ) -> PluginResult<()> {
        let url = self.endpoint(&format!("/applications/{}/sync", app_name))?;

        let strategy = sync_strategy(force, apply_only);

        let sync_request = SyncRequest {
            revision,
            prune: Some(prune),
            dry_run: Some(dry_run),
            strategy,
        };

        debug!(?sync_request, "Sending sync request to ArgoCD API");

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .json(&sync_request)
                    .send()
            })
            .await?;

        check_response(response).await?;
        Ok(())
    }

    pub async fn terminate_operation(&self, app_name: &str) -> PluginResult<()> {
        let url = self.endpoint(&format!("/applications/{}/operation", app_name))?;

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .delete(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .send()
            })
            .await?;

        check_response(response).await?;
        Ok(())
    }
}

fn sync_strategy(force: bool, apply_only: bool) -> Option<SyncStrategy> {
    if apply_only {
        Some(SyncStrategy {
            apply: Some(SyncStrategyApply { force: Some(force) }),
            hook: None,
        })
    } else if force {
        Some(SyncStrategy {
            apply: None,
            hook: Some(SyncStrategyHook { force: Some(true) }),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn strategy_json(force: bool, apply_only: bool) -> serde_json::Value {
        serde_json::to_value(sync_strategy(force, apply_only)).unwrap()
    }

    #[test]
    fn apply_only_sync_skips_hooks() {
        assert_eq!(
            strategy_json(false, true),
            json!({ "apply": { "force": false }, "hook": null })
        );
        assert_eq!(
            strategy_json(true, true),
            json!({ "apply": { "force": true }, "hook": null })
        );
    }

    #[test]
    fn force_sync_keeps_hooks() {
        assert_eq!(
            strategy_json(true, false),
            json!({ "apply": null, "hook": { "force": true } })
        );
    }

    #[test]
    fn plain_sync_sends_no_strategy() {
        assert_eq!(strategy_json(false, false), serde_json::Value::Null);
    }
}
