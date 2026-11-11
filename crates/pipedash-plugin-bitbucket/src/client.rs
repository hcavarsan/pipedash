use pipedash_plugin_api::http::{
    decode_json,
    get_json,
    parse_http_url,
    send,
    send_checked,
};
use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::utils::Cached;
use pipedash_plugin_api::{
    check_response,
    LogReadMode,
    LogSink,
    PaginatedResponse,
    PaginationParams,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::{
    Method,
    Url,
};

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
    user_cache: Cached<User>,
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
            user_cache: Cached::default(),
        }
    }

    pub async fn get_user(&self) -> PluginResult<User> {
        let url = format!("{}/user", self.api_url);
        self.user_cache.get_or_fetch(|| self.get_json(&url)).await
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

            let paginated: BitbucketPaginatedResponse<Workspace> = self.get_json(&url).await?;

            all_workspaces.extend(paginated.values);
            next_url = paginated.next;
            page_count += 1;
        }

        Ok(all_workspaces)
    }

    pub async fn list_repositories(
        &self, workspace: &str, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        let url = format!(
            "{}/repositories/{}?pagelen={}&page={}",
            self.api_url, workspace, params.page_size, params.page
        );
        self.repository_page(&url, params).await
    }

    pub async fn list_all_repositories(
        &self, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        let url = format!(
            "{}/repositories?role=member&pagelen={}&page={}",
            self.api_url, params.page_size, params.page
        );
        self.repository_page(&url, params).await
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

        while let Some(url) = next_url.take() {
            if pipelines.len() >= limit {
                break;
            }

            let paginated: BitbucketPaginatedResponse<Pipeline> = self.get_json(&url).await?;

            pipelines.extend(paginated.values);
            next_url = paginated.next;
        }

        pipelines.truncate(limit);
        Ok(pipelines)
    }

    pub async fn has_pipelines_file(&self, workspace: &str, repo_slug: &str) -> PluginResult<bool> {
        let url = Self::parse_url(&format!(
            "{}/repositories/{}/{}/src/HEAD/bitbucket-pipelines.yml",
            self.api_url, workspace, repo_slug
        ))?;
        let found = self
            .retry_policy
            .retry(|| async {
                check_response(send(&url, self.authorized(Method::GET, &url)).await?).await
            })
            .await;
        match found {
            Ok(_) => Ok(true),
            Err(PluginError::PipelineNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn find_pipeline_by_build_number(
        &self, workspace: &str, repo_slug: &str, build_number: i64,
    ) -> PluginResult<Pipeline> {
        let mut next_url = Some(format!(
            "{}/repositories/{}/{}/pipelines?pagelen=100&sort=-created_on",
            self.api_url, workspace, repo_slug
        ));

        while let Some(url) = next_url.take() {
            let paginated: BitbucketPaginatedResponse<Pipeline> = self.get_json(&url).await?;

            if let Some(pipeline) = paginated
                .values
                .into_iter()
                .find(|pipeline| pipeline.build_number == build_number)
            {
                return Ok(pipeline);
            }
            next_url = paginated.next;
        }

        Err(PluginError::PipelineNotFound(format!(
            "Pipeline run #{build_number} not found for {workspace}/{repo_slug}"
        )))
    }

    pub async fn list_steps(
        &self, workspace: &str, repo_slug: &str, pipeline_uuid: &str,
    ) -> PluginResult<Vec<PipelineStep>> {
        let mut steps = Vec::new();
        let mut next_url = Some(format!(
            "{}/repositories/{}/{}/pipelines/{}/steps?pagelen=100",
            self.api_url,
            workspace,
            repo_slug,
            Self::encoded_uuid(pipeline_uuid)
        ));
        const MAX_PAGES: usize = 10;
        let mut page_count = 0;

        while let Some(url) = next_url.take() {
            if page_count >= MAX_PAGES {
                break;
            }

            let paginated: BitbucketPaginatedResponse<PipelineStep> = self.get_json(&url).await?;

            steps.extend(paginated.values);
            next_url = paginated.next;
            page_count += 1;
        }

        Ok(steps)
    }

    pub async fn stream_step_log(
        &self, workspace: &str, repo_slug: &str, pipeline_uuid: &str, step_uuid: &str,
        offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let url = Self::parse_url(&format!(
            "{}/repositories/{}/{}/pipelines/{}/steps/{}/log",
            self.api_url,
            workspace,
            repo_slug,
            Self::encoded_uuid(pipeline_uuid),
            Self::encoded_uuid(step_uuid)
        ))?;

        let url = &url;
        let gone = format!("Log of step {step_uuid} is no longer available");
        sink.pipe_resumed(offset, |from| {
            send_log(&self.retry_policy, url, from, &gone, || {
                self.authorized(Method::GET, url)
            })
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
        let response = send_checked(&self.retry_policy, &url, || {
            self.authorized(Method::POST, &url).json(&request)
        })
        .await?;

        decode_json(response, "Bitbucket API response").await
    }

    pub async fn stop_pipeline(
        &self, workspace: &str, repo_slug: &str, pipeline_uuid: &str,
    ) -> PluginResult<()> {
        let url = Self::parse_url(&format!(
            "{}/repositories/{}/{}/pipelines/{}/stopPipeline",
            self.api_url,
            workspace,
            repo_slug,
            Self::encoded_uuid(pipeline_uuid)
        ))?;
        send_checked(&self.retry_policy, &url, || {
            self.authorized(Method::POST, &url)
        })
        .await
        .map(drop)
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
        parse_http_url(url, "Bitbucket")
    }

    fn encoded_uuid(uuid: &str) -> String {
        if uuid.starts_with('{') {
            urlencoding::encode(uuid).into_owned()
        } else {
            urlencoding::encode(&format!("{{{uuid}}}")).into_owned()
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> PluginResult<T> {
        let url = Self::parse_url(url)?;
        get_json(&self.retry_policy, &url, "Bitbucket API response", || {
            self.authorized(Method::GET, &url)
        })
        .await
    }

    fn authorized(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .header(reqwest::header::AUTHORIZATION, &self.auth_value)
    }
}

#[cfg(test)]
mod tests;
