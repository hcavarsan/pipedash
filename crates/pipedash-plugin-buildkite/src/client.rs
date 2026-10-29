use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    AvailablePipeline,
    BuildArtifact,
    ContentRange,
    LogReadMode,
    LogSink,
    PaginatedResponse,
    PaginationParams,
    Pipeline,
    PipelineRun,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::header::{
    ACCEPT,
    ACCEPT_ENCODING,
    AUTHORIZATION,
    CONTENT_LENGTH,
    RANGE,
};
use reqwest::{
    Client,
    Method,
    Url,
};
use serde::de::DeserializeOwned;

use crate::{
    config,
    mapper,
    types,
};

const BASE_URL: &str = "https://api.buildkite.com/v2";
const MAX_RESUME_ATTEMPTS: usize = 3;

fn parse_url(url: &str) -> PluginResult<Url> {
    Url::parse(url).map_err(|e| PluginError::InvalidConfig(format!("Invalid Buildkite URL: {e}")))
}

pub(crate) struct BuildkiteClient {
    http_client: std::sync::Arc<Client>,
    token: String,
    retry_policy: RetryPolicy,
    base_url: String,
}

impl BuildkiteClient {
    pub fn new(http_client: std::sync::Arc<Client>, token: String) -> Self {
        Self {
            http_client,
            token,
            retry_policy: RetryPolicy::default(),
            base_url: BASE_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> PluginResult<T> {
        let url = parse_url(url)?;

        self.retry_policy
            .retry(|| async {
                let response = self
                    .http_client
                    .get(url.clone())
                    .header(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", self.token),
                    )
                    .send()
                    .await
                    .map_err(|e| map_send_error(e, &url))?;

                check_response(response)
                    .await?
                    .json()
                    .await
                    .map_err(|e| map_send_error(e, &url))
            })
            .await
    }

    pub async fn fetch_organizations(&self) -> PluginResult<Vec<types::Organization>> {
        self.get_json(&format!("{}/organizations", self.base_url))
            .await
    }

    pub async fn fetch_org_pipelines(
        &self, org_slug: String, page: usize, per_page: usize,
    ) -> PluginResult<Vec<AvailablePipeline>> {
        let pipelines: Vec<types::Pipeline> = self
            .get_json(&format!(
                "{}/organizations/{org_slug}/pipelines?per_page={per_page}&page={page}",
                self.base_url
            ))
            .await?;

        Ok(pipelines
            .into_iter()
            .map(|pipeline| {
                let repo_name = config::parse_repository_name(&pipeline.repository);
                AvailablePipeline {
                    id: format!("{org_slug}/{}", pipeline.name),
                    name: pipeline.name,
                    description: Some(repo_name.clone()),
                    organization: Some(org_slug.clone()),
                    repository: Some(repo_name),
                }
            })
            .collect())
    }

    pub async fn fetch_pipeline(
        &self, provider_id: i64, org: String, slug: String,
    ) -> PluginResult<Pipeline> {
        let pipeline: types::Pipeline = self
            .get_json(&format!(
                "{}/organizations/{org}/pipelines/{slug}",
                self.base_url
            ))
            .await?;

        let builds: Vec<types::Build> = self
            .get_json(&format!(
                "{}/organizations/{org}/pipelines/{slug}/builds?per_page=1",
                self.base_url
            ))
            .await?;

        let latest_build = builds.first();
        let status = latest_build
            .map(|build| mapper::map_build_state(&build.state))
            .unwrap_or(pipedash_plugin_api::PipelineStatus::Pending);

        let last_run = latest_build.and_then(|build| {
            chrono::DateTime::parse_from_rfc3339(&build.created_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        let mut metadata = HashMap::new();
        metadata.insert(
            "organization_slug".to_string(),
            serde_json::json!(org.clone()),
        );

        Ok(Pipeline {
            id: format!("buildkite__{provider_id}__{org}__{slug}"),
            provider_id,
            provider_type: "buildkite".to_string(),
            name: pipeline.name,
            status,
            last_run,
            last_updated: Utc::now(),
            repository: config::parse_repository_name(&pipeline.repository),
            branch: latest_build.and_then(|b| {
                if b.branch.is_empty() {
                    None
                } else {
                    Some(b.branch.clone())
                }
            }),
            workflow_file: None,
            resource_state: None,
            metadata,
        })
    }

    pub async fn fetch_builds(
        &self, org: &str, slug: &str, limit: usize,
    ) -> PluginResult<Vec<types::Build>> {
        let per_page = 100;
        let total_pages = limit.div_ceil(100);
        let mut all_builds = Vec::new();

        for page in 1..=total_pages.min(10) {
            let builds: Vec<types::Build> = self
                .get_json(&format!(
                    "{}/organizations/{org}/pipelines/{slug}/builds?per_page={}&page={}",
                    self.base_url, per_page, page
                ))
                .await?;

            if builds.is_empty() {
                break;
            }

            all_builds.extend(builds);

            if all_builds.len() >= limit {
                break;
            }
        }

        all_builds.truncate(limit);
        Ok(all_builds)
    }

    pub async fn fetch_build(
        &self, org: &str, slug: &str, number: i64,
    ) -> PluginResult<types::Build> {
        self.get_json(&format!(
            "{}/organizations/{org}/pipelines/{slug}/builds/{number}?exclude_jobs=true",
            self.base_url
        ))
        .await
    }

    pub async fn fetch_build_jobs(
        &self, org: &str, slug: &str, number: i64,
    ) -> PluginResult<Vec<types::Job>> {
        let build: types::BuildJobs = self
            .get_json(&format!(
                "{}/organizations/{org}/pipelines/{slug}/builds/{number}",
                self.base_url
            ))
            .await?;

        Ok(build.jobs)
    }

    pub async fn stream_job_log(
        &self, org: &str, slug: &str, number: i64, job_id: &str, offset: Option<u64>,
        sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let url = parse_url(&format!(
            "{}/organizations/{org}/pipelines/{slug}/builds/{number}/jobs/{job_id}/log",
            self.base_url
        ))?;

        if let Some(offset) = offset {
            if let Some(next) = self.append_job_log(&url, job_id, offset, sink).await? {
                return Ok((LogReadMode::Append, next));
            }
            sink.restart()?;
        }

        let response = self
            .send_log(&url, job_id, || self.log_request(Method::GET, &url))
            .await?;
        sink.pipe(check_response(response).await?).await?;
        Ok((LogReadMode::Replace, sink.bytes_written()))
    }

    async fn append_job_log(
        &self, url: &Url, job_id: &str, offset: u64, sink: &mut LogSink,
    ) -> PluginResult<Option<u64>> {
        let head = self
            .send_log(url, job_id, || {
                self.log_request(Method::HEAD, url)
                    .header(ACCEPT_ENCODING, "identity")
            })
            .await?;
        let head = check_response(head).await?;
        let Some(size) = head
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|size| *size >= offset)
        else {
            return Ok(None);
        };
        if size == offset {
            return Ok(Some(offset));
        }

        let mut unseen = size - offset;
        for _ in 0..MAX_RESUME_ATTEMPTS {
            let response = self
                .send_log(url, job_id, || {
                    self.log_request(Method::GET, url)
                        .header(ACCEPT_ENCODING, "identity")
                        .header(RANGE, format!("bytes=-{unseen}"))
                })
                .await?;
            if response.status().as_u16() == 416 {
                return Ok(None);
            }
            let response = check_response(response).await?;
            let Some(((start, _), total)) = ContentRange::parse(response.headers())
                .and_then(|range| Some((range.span?, range.total?)))
                .filter(|(_, total)| *total >= offset && response.status().as_u16() == 206)
            else {
                return Ok(None);
            };
            if start > offset {
                unseen = total - offset;
                continue;
            }
            sink.pipe_after(response, offset - start).await?;
            return Ok(Some(offset + sink.bytes_written()));
        }
        Ok(None)
    }

    fn log_request(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(ACCEPT, "text/plain")
            .timeout(Duration::from_secs(900))
    }

    async fn send_log(
        &self, url: &Url, job_id: &str, request: impl Fn() -> reqwest::RequestBuilder,
    ) -> PluginResult<reqwest::Response> {
        let response = self
            .retry_policy
            .retry_unsent(url, || request().send())
            .await?;

        if matches!(response.status().as_u16(), 404 | 410) {
            return Err(PluginError::PipelineNotFound(format!(
                "Logs for job {job_id} are no longer available"
            )));
        }
        Ok(response)
    }

    pub async fn trigger_build(
        &self, org: &str, slug: &str, branch: String, inputs: Option<serde_json::Value>,
    ) -> PluginResult<types::Build> {
        let url = parse_url(&format!(
            "{}/organizations/{org}/pipelines/{slug}/builds",
            self.base_url
        ))?;

        let mut body = serde_json::json!({
            "branch": branch,
            "commit": "HEAD",
        });

        if let Some(obj) = inputs.as_ref().and_then(|inputs| inputs.as_object()) {
            if let Some(message) = obj.get("message") {
                body["message"] = message.clone();
            }
            if let Some(env) = obj.get("env") {
                body["env"] = env.clone();
            }
        }

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", self.token),
                    )
                    .json(&body)
                    .send()
            })
            .await?;

        check_response(response)
            .await?
            .json()
            .await
            .map_err(|e| map_send_error(e, &url))
    }

    pub async fn fetch_agents(&self, org: &str) -> PluginResult<Vec<types::Agent>> {
        self.get_json(&format!("{}/organizations/{org}/agents", self.base_url))
            .await
    }

    pub async fn fetch_artifacts(
        &self, org: &str, build_id: &str,
    ) -> PluginResult<Vec<types::Artifact>> {
        self.get_json(&format!(
            "{}/organizations/{org}/builds/{build_id}/artifacts",
            self.base_url
        ))
        .await
    }

    pub async fn cancel_build(
        &self, org: &str, pipeline_slug: &str, build_number: i64,
    ) -> PluginResult<()> {
        let url = parse_url(&format!(
            "{}/organizations/{org}/pipelines/{pipeline_slug}/builds/{build_number}/cancel",
            self.base_url
        ))?;

        tracing::info!(
            build_number = build_number,
            org = %org,
            pipeline = %pipeline_slug,
            "Cancelling Buildkite build"
        );

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .put(url.clone())
                    .header(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", self.token),
                    )
                    .send()
            })
            .await?;
        check_response(response).await?;

        tracing::info!(
            build_number = build_number,
            "Buildkite build cancelled successfully"
        );
        Ok(())
    }
}

pub(crate) fn build_to_pipeline_run(build: types::Build, pipeline_id: &str) -> PipelineRun {
    let status = mapper::map_build_state(&build.state);

    let started_at = build
        .started_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .or_else(|| chrono::DateTime::parse_from_rfc3339(&build.created_at).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let concluded_at = build
        .finished_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let duration_seconds = if let (Some(start), Some(finish)) =
        (build.started_at.as_ref(), build.finished_at.as_ref())
    {
        let start_dt = chrono::DateTime::parse_from_rfc3339(start).ok();
        let finish_dt = chrono::DateTime::parse_from_rfc3339(finish).ok();
        if let (Some(s), Some(f)) = (start_dt, finish_dt) {
            Some((f - s).num_seconds())
        } else {
            None
        }
    } else {
        None
    };

    let mut inputs_map = serde_json::Map::new();

    let branch_value = if build.branch.is_empty() {
        "unknown".to_string()
    } else {
        build.branch.clone()
    };

    inputs_map.insert(
        "branch".to_string(),
        serde_json::Value::String(branch_value.clone()),
    );
    inputs_map.insert(
        "commit".to_string(),
        serde_json::Value::String(build.commit.clone()),
    );

    tracing::debug!(
        build_number = build.number,
        branch = %branch_value,
        commit = %build.commit,
        "Processing Buildkite build"
    );

    let inputs = Some(serde_json::Value::Object(inputs_map));

    PipelineRun {
        id: format!("{pipeline_id}__{}", build.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: build.number,
        status,
        started_at,
        concluded_at,
        duration_seconds,
        logs_url: build.web_url.clone(),
        commit_sha: if build.commit.is_empty() {
            None
        } else {
            Some(build.commit.clone())
        },
        commit_message: build.message.clone(),
        branch: Some(branch_value),
        actor: build.author.as_ref().map(|a| a.name.clone()),
        inputs,
        metadata: HashMap::new(), // No additional metadata for runs yet
    }
}

pub(crate) fn artifact_to_build_artifact(artifact: types::Artifact, run_id: &str) -> BuildArtifact {
    BuildArtifact {
        id: artifact.id,
        run_id: run_id.to_string(),
        filename: artifact.filename,
        size_bytes: artifact.size,
        download_url: artifact.download_url,
        content_type: None,
        created_at: Utc::now(),
    }
}

pub(crate) async fn fetch_all_available_pipelines(
    client: &BuildkiteClient, params: Option<PaginationParams>,
) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
    let params = params.unwrap_or_default();
    let organizations = client.fetch_organizations().await?;

    let mut all_available_pipelines = Vec::new();

    for org in organizations {
        let mut page = 1;
        loop {
            let pipelines = client
                .fetch_org_pipelines(org.slug.clone(), page, 100)
                .await;
            match pipelines {
                Ok(p) => {
                    if p.is_empty() {
                        break;
                    }
                    let is_last_page = p.len() < 100;
                    all_available_pipelines.extend(p);
                    if is_last_page {
                        break;
                    }
                    page += 1;
                }
                Err(e) if e.is_transient() => return Err(e),
                Err(e) => {
                    tracing::warn!(org = %org.slug, error = %e, "Failed to fetch Buildkite pipelines for org");
                    break;
                }
            }
        }
    }

    let total_count = all_available_pipelines.len();
    let start = params.calculate_offset().unwrap_or(0).min(total_count);
    let end = (start + params.page_size).min(total_count);
    let items = all_available_pipelines[start..end].to_vec();

    Ok(PaginatedResponse::new(
        items,
        params.page,
        params.page_size,
        total_count,
    ))
}

pub(crate) async fn fetch_available_pipelines_filtered(
    client: &BuildkiteClient, org: Option<String>, search: Option<String>,
    params: Option<PaginationParams>,
) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
    let params = params.unwrap_or_default();

    let mut all_pipelines = if let Some(org_slug) = org {
        let mut all_from_org = Vec::new();
        let mut page = 1;
        loop {
            let pipelines = client
                .fetch_org_pipelines(org_slug.clone(), page, 100)
                .await?;
            if pipelines.is_empty() {
                break;
            }
            let is_last_page = pipelines.len() < 100;
            all_from_org.extend(pipelines);
            if is_last_page {
                break;
            }
            page += 1;
        }
        all_from_org
    } else {
        let organizations = client.fetch_organizations().await?;
        let mut all_available_pipelines = Vec::new();

        for org in organizations {
            let mut page = 1;
            loop {
                let pipelines = client
                    .fetch_org_pipelines(org.slug.clone(), page, 100)
                    .await;
                match pipelines {
                    Ok(p) => {
                        if p.is_empty() {
                            break;
                        }
                        let is_last_page = p.len() < 100;
                        all_available_pipelines.extend(p);
                        if is_last_page {
                            break;
                        }
                        page += 1;
                    }
                    Err(e) if e.is_transient() => return Err(e),
                    Err(e) => {
                        tracing::warn!(org = %org.slug, error = %e, "Failed to fetch Buildkite pipelines for org");
                        break;
                    }
                }
            }
        }
        all_available_pipelines
    };

    if let Some(search_term) = search {
        let search_lower = search_term.to_lowercase();
        all_pipelines.retain(|p| {
            p.name.to_lowercase().contains(&search_lower)
                || p.id.to_lowercase().contains(&search_lower)
                || p.description
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&search_lower))
        });
    }

    let total_count = all_pipelines.len();
    let start = params.calculate_offset().unwrap_or(0).min(total_count);
    let end = (start + params.page_size).min(total_count);
    let items = all_pipelines[start..end].to_vec();

    Ok(PaginatedResponse::new(
        items,
        params.page,
        params.page_size,
        total_count,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::test_support::{
        stub_server,
        Reply,
    };

    fn unauthorized(_: &str) -> Reply {
        Reply {
            status: "401 Unauthorized",
            headers: "Content-Type: application/json\r\n",
            body: r#"{"message":"Authentication required. Please supply a valid API Access Token"}"#,
        }
    }

    fn not_found(_: &str) -> Reply {
        Reply {
            status: "404 Not Found",
            headers: "",
            body: r#"{"message":"No pipeline found"}"#,
        }
    }

    fn rate_limited(_: &str) -> Reply {
        Reply {
            status: "429 Too Many Requests",
            headers: "Retry-After: 120\r\n",
            body: "slow down",
        }
    }

    fn unavailable(_: &str) -> Reply {
        Reply {
            status: "503 Service Unavailable",
            headers: "",
            body: "maintenance",
        }
    }

    fn created(_: &str) -> Reply {
        Reply {
            status: "201 Created",
            headers: "Content-Type: application/json\r\n",
            body: "{}",
        }
    }

    async fn client_for(
        reply: fn(&str) -> Reply, delay: Duration,
    ) -> (BuildkiteClient, Arc<AtomicUsize>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(AtomicUsize::new(0));
        let base_url = stub_server(requests.clone(), delay, reply).await;
        let http_client = Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let client = BuildkiteClient::new(Arc::new(http_client), "token".to_string())
            .with_base_url(base_url);
        (client, requests)
    }

    #[tokio::test]
    async fn rejected_token_fails_as_auth_error_without_retry() {
        let (client, requests) = client_for(unauthorized, Duration::ZERO).await;

        let results = [
            client.fetch_organizations().await.err(),
            client
                .fetch_org_pipelines("acme".to_string(), 1, 10)
                .await
                .err(),
            client
                .fetch_pipeline(1, "acme".to_string(), "app".to_string())
                .await
                .err(),
            client.fetch_builds("acme", "app", 10).await.err(),
            client.fetch_agents("acme").await.err(),
            client.fetch_artifacts("acme", "build-1").await.err(),
        ];

        for error in results {
            assert!(
                matches!(error, Some(PluginError::AuthenticationFailed(_))),
                "{error:?}"
            );
        }
        assert_eq!(requests.load(Ordering::SeqCst), 6);
    }

    #[tokio::test]
    async fn missing_pipeline_is_pipeline_not_found() {
        let (client, requests) = client_for(not_found, Duration::ZERO).await;

        let error = client
            .fetch_pipeline(1, "acme".to_string(), "gone".to_string())
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{error:?}"
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rate_limit_carries_retry_after_and_is_not_retried() {
        let (client, requests) = client_for(rate_limited, Duration::ZERO).await;

        let error = client.fetch_organizations().await.unwrap_err();

        assert!(
            matches!(error, PluginError::RateLimited { .. }),
            "{error:?}"
        );
        assert_eq!(error.retry_after(), Some(Duration::from_secs(120)));
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn server_error_is_service_unavailable_and_retried() {
        let (client, requests) = client_for(unavailable, Duration::ZERO).await;

        let error = client.fetch_organizations().await.unwrap_err();

        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error:?}"
        );
        assert_eq!(requests.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn rate_limited_mutations_map_status_without_resending() {
        let (client, requests) = client_for(rate_limited, Duration::ZERO).await;

        let trigger = client
            .trigger_build("acme", "app", "main".to_string(), None)
            .await
            .unwrap_err();
        let cancel = client.cancel_build("acme", "app", 7).await.unwrap_err();

        assert!(
            matches!(trigger, PluginError::RateLimited { .. }),
            "{trigger:?}"
        );
        assert!(
            matches!(cancel, PluginError::RateLimited { .. }),
            "{cancel:?}"
        );
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn timed_out_mutations_are_not_sent_again() {
        let (client, requests) = client_for(created, Duration::from_secs(2)).await;

        let trigger = client
            .trigger_build("acme", "app", "main".to_string(), None)
            .await
            .unwrap_err();
        let cancel = client.cancel_build("acme", "app", 7).await.unwrap_err();

        assert!(
            matches!(trigger, PluginError::NetworkError(_)),
            "{trigger:?}"
        );
        assert!(matches!(cancel, PluginError::NetworkError(_)), "{cancel:?}");
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }
}
