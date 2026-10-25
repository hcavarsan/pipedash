use std::sync::OnceLock;

use pipedash_plugin_api::{
    check_response,
    map_send_error,
    redact_url,
    PaginatedResponse,
    PaginationParams,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::Url;

use crate::types::{
    PaginatedResponse as BitbucketPaginatedResponse,
    Pipeline,
    PipelineStep,
    Repository,
    TriggerPipelineRequest,
    User,
    Workspace,
};

pub struct BitbucketClient {
    http_client: std::sync::Arc<reqwest::Client>,
    api_url: String,
    auth_value: String,
    retry_policy: RetryPolicy,
    user_cache: OnceLock<User>,
}

impl BitbucketClient {
    pub fn new(
        http_client: std::sync::Arc<reqwest::Client>, api_url: String, auth_value: String,
    ) -> Self {
        Self {
            http_client,
            api_url: api_url.trim_end_matches('/').to_string(),
            auth_value,
            retry_policy: RetryPolicy::default(),
            user_cache: OnceLock::new(),
        }
    }

    pub async fn get_user(&self) -> PluginResult<User> {
        if let Some(user) = self.user_cache.get() {
            return Ok(user.clone());
        }

        let user: User = self
            .retry_policy
            .retry(|| async { self.get_json(&format!("{}/user", self.api_url)).await })
            .await?;

        let _ = self.user_cache.set(user.clone());
        Ok(user)
    }

    pub async fn list_workspaces(&self) -> PluginResult<Vec<Workspace>> {
        let mut all_workspaces = Vec::new();
        let mut next_url = Some(format!("{}/workspaces?pagelen=100", self.api_url));
        const MAX_PAGES: usize = 10;
        let mut page_count = 0;

        while let Some(url) = next_url.take() {
            if page_count >= MAX_PAGES {
                break;
            }

            let paginated: BitbucketPaginatedResponse<Workspace> = self
                .retry_policy
                .retry(|| async { self.get_json(&url).await })
                .await?;

            all_workspaces.extend(paginated.values);
            next_url = paginated.next;
            page_count += 1;
        }

        Ok(all_workspaces)
    }

    pub async fn list_repositories(
        &self, workspace: &str, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories/{}?pagelen={}&page={}",
                    self.api_url, workspace, params.page_size, params.page
                );
                self.repository_page(&url, params).await
            })
            .await
    }

    pub async fn list_all_repositories(
        &self, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories?role=member&pagelen={}&page={}",
                    self.api_url, params.page_size, params.page
                );
                self.repository_page(&url, params).await
            })
            .await
    }

    pub async fn list_pipelines(
        &self, workspace: &str, repo_slug: &str, limit: usize,
    ) -> PluginResult<Vec<Pipeline>> {
        let mut pipelines = Vec::new();
        let mut next_url = Some(format!(
            "{}/repositories/{}/{}/pipelines?pagelen={}&sort=-created_on",
            self.api_url,
            workspace,
            repo_slug,
            limit.min(100)
        ));
        const MAX_PAGES: usize = 10;
        let mut page_count = 0;

        while let Some(url) = next_url.take() {
            if page_count >= MAX_PAGES || pipelines.len() >= limit {
                break;
            }

            let paginated: BitbucketPaginatedResponse<Pipeline> = self
                .retry_policy
                .retry(|| async { self.get_json(&url).await })
                .await?;

            pipelines.extend(paginated.values);
            next_url = paginated.next;
            page_count += 1;
        }

        pipelines.truncate(limit);
        Ok(pipelines)
    }

    pub async fn list_steps(
        &self, workspace: &str, repo_slug: &str, pipeline_uuid: &str,
    ) -> PluginResult<Vec<PipelineStep>> {
        let uuid_with_braces = if pipeline_uuid.starts_with('{') {
            pipeline_uuid.to_string()
        } else {
            format!("{{{}}}", pipeline_uuid)
        };
        let encoded_uuid = urlencoding::encode(&uuid_with_braces);

        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories/{}/{}/pipelines/{}/steps",
                    self.api_url, workspace, repo_slug, encoded_uuid
                );
                let paginated: BitbucketPaginatedResponse<PipelineStep> =
                    self.get_json(&url).await?;
                Ok(paginated.values)
            })
            .await
    }

    pub async fn trigger_pipeline(
        &self, workspace: &str, repo_slug: &str, request: TriggerPipelineRequest,
    ) -> PluginResult<Pipeline> {
        let url = Self::parse_url(&format!(
            "{}/repositories/{}/{}/pipelines/",
            self.api_url, workspace, repo_slug
        ))?;
        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_value)
                    .json(&request)
                    .send()
            })
            .await?;

        Self::json(check_response(response).await?).await
    }

    pub async fn stop_pipeline(
        &self, workspace: &str, repo_slug: &str, pipeline_uuid: &str,
    ) -> PluginResult<()> {
        let uuid_with_braces = if pipeline_uuid.starts_with('{') {
            pipeline_uuid.to_string()
        } else {
            format!("{{{}}}", pipeline_uuid)
        };
        let encoded_uuid = urlencoding::encode(&uuid_with_braces);

        let url = Self::parse_url(&format!(
            "{}/repositories/{}/{}/pipelines/{}/stopPipeline",
            self.api_url, workspace, repo_slug, encoded_uuid
        ))?;
        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_value)
                    .send()
            })
            .await?;

        check_response(response).await.map(drop)
    }

    async fn repository_page(
        &self, url: &str, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        let paginated: BitbucketPaginatedResponse<Repository> = self.get_json(url).await?;
        let total_count = paginated.size.unwrap_or(paginated.values.len());
        Ok(PaginatedResponse::new(
            paginated.values,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    fn parse_url(url: &str) -> PluginResult<Url> {
        Url::parse(url)
            .map_err(|e| PluginError::InvalidConfig(format!("Invalid Bitbucket URL: {e}")))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> PluginResult<T> {
        let url = Self::parse_url(url)?;
        let response = self
            .http_client
            .get(url.clone())
            .header(reqwest::header::AUTHORIZATION, &self.auth_value)
            .send()
            .await
            .map_err(|e| map_send_error(e, &url))?;
        Self::json(check_response(response).await?).await
    }

    async fn json<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> PluginResult<T> {
        let url = response.url().clone();
        response.json::<T>().await.map_err(|e| {
            PluginError::ApiError(format!(
                "Failed to parse Bitbucket API response from {}: {}",
                redact_url(&url),
                e.without_url()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{
        Read,
        Write,
    };
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use std::sync::Arc;

    use super::*;

    fn page(from: i64, count: i64, next: Option<String>) -> String {
        let values: Vec<serde_json::Value> = (from..from + count)
            .rev()
            .map(|n| {
                serde_json::json!({
                    "uuid": format!("{{{n}}}"),
                    "build_number": n,
                    "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}},
                    "target": {"type": "pipeline_ref_target", "ref_name": "main"},
                    "created_on": "2026-10-01T00:00:00Z",
                    "links": {}
                })
            })
            .collect();
        serde_json::json!({"values": values, "next": next, "pagelen": 100}).to_string()
    }

    fn serve_two_pages() -> (BitbucketClient, Arc<AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = request.lines().next().unwrap_or_default().to_string();
                counter.fetch_add(1, Ordering::SeqCst);
                let body = if line.contains("page=2") {
                    page(1, 50, None)
                } else {
                    page(
                        51,
                        100,
                        Some(format!(
                            "http://127.0.0.1:{port}/repositories/ws/repo/pipelines?pagelen=100&sort=-created_on&page=2"
                        )),
                    )
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = BitbucketClient::new(
            Arc::new(reqwest::Client::new()),
            format!("http://127.0.0.1:{port}"),
            "Basic x".to_string(),
        );
        (client, requests)
    }

    #[tokio::test]
    async fn run_history_follows_next_past_the_first_100_runs() {
        let (client, requests) = serve_two_pages();

        let runs = client.list_pipelines("ws", "repo", 200).await.unwrap();

        assert_eq!(runs.len(), 150);
        assert_eq!(runs.first().map(|run| run.build_number), Some(150));
        assert_eq!(runs.last().map(|run| run.build_number), Some(1));
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn run_history_stops_at_the_limit() {
        let (client, requests) = serve_two_pages();

        let runs = client.list_pipelines("ws", "repo", 120).await.unwrap();
        assert_eq!(runs.len(), 120);
        assert_eq!(runs.last().map(|run| run.build_number), Some(31));
        assert_eq!(requests.load(Ordering::SeqCst), 2);

        let runs = client.list_pipelines("ws", "repo", 100).await.unwrap();
        assert_eq!(runs.len(), 100);
        assert_eq!(requests.load(Ordering::SeqCst), 3);
    }
}
