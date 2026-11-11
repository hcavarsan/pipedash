use std::sync::Arc;

use pipedash_plugin_api::http::{
    bearer_request,
    decode_json,
    parse_http_url,
    send_checked,
};
use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    LogSink,
    PluginError,
    PluginResult,
    RetryPolicy,
    LOG_CHUNK_STALL_TIMEOUT,
};
use reqwest::{
    Method,
    Url,
};
use serde::de::DeserializeOwned;
use tracing::debug;

use crate::logs::decoder::PodLogDecoder;
use crate::logs::POD_LOG_TAIL_LINES;
use crate::types::logs::{
    ApplicationTree,
    Event,
    EventList,
    Pod,
    ResourceManifest,
};
use crate::types::{
    Application,
    ApplicationList,
    Settings,
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
    token: String,
    retry_policy: RetryPolicy,
}

impl ArgocdClient {
    pub fn new(http_client: Arc<reqwest::Client>, server_url: &str, token: &str) -> Self {
        Self {
            http_client,
            api_url: Self::build_api_url(server_url),
            token: token.to_string(),
            retry_policy: RetryPolicy::default(),
        }
    }

    fn build_api_url(server_url: &str) -> String {
        format!("{}/api/v1", server_url.trim_end_matches('/'))
    }

    pub async fn fetch_settings(
        http_client: &reqwest::Client, server_url: &str,
    ) -> PluginResult<Settings> {
        let url = parse_http_url(
            &format!("{}/settings", Self::build_api_url(server_url)),
            "ArgoCD",
        )?;
        read_json(http_client.get(url.clone()), &url).await
    }

    fn endpoint(&self, path: &str) -> PluginResult<Url> {
        parse_http_url(&format!("{}{}", self.api_url, path), "ArgoCD")
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &Url) -> PluginResult<T> {
        self.retry_policy
            .retry(|| async {
                read_json(
                    bearer_request(&self.http_client, Method::GET, url, &self.token),
                    url,
                )
                .await
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

        let gone = format!("Logs of pod {} are no longer available", request.pod);
        let response = send_log(&self.retry_policy, &url, None, &gone, || {
            bearer_request(&self.http_client, Method::GET, &url, &self.token)
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
        &self, app_name: &str, sync_request: &SyncRequest,
    ) -> PluginResult<()> {
        let url = self.endpoint(&format!("/applications/{}/sync", app_name))?;

        debug!(?sync_request, "Sending sync request to ArgoCD API");

        send_checked(&self.retry_policy, &url, || {
            bearer_request(&self.http_client, Method::POST, &url, &self.token).json(sync_request)
        })
        .await
        .map(drop)
    }

    pub async fn terminate_operation(&self, app_name: &str) -> PluginResult<()> {
        let url = self.endpoint(&format!("/applications/{}/operation", app_name))?;

        send_checked(&self.retry_policy, &url, || {
            bearer_request(&self.http_client, Method::DELETE, &url, &self.token)
        })
        .await
        .map(drop)
    }
}

async fn read_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder, url: &Url,
) -> PluginResult<T> {
    let response = request.send().await.map_err(|e| map_send_error(e, url))?;
    decode_json(check_response(response).await?, "ArgoCD API response").await
}

pub(crate) fn sync_strategy(force: bool, apply_only: bool) -> Option<SyncStrategy> {
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
mod tests;
