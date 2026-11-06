use std::collections::HashSet;
use std::sync::Arc;

use pipedash_plugin_api::{
    http,
    PluginError,
    PluginResult,
};
use reqwest::{
    header::HeaderValue,
    Method,
    Url,
};
use serde::de::DeserializeOwned;
use serde_json::{
    json,
    Value,
};

use crate::types::{
    CirclePipeline,
    FollowedProject,
    Page,
    Project,
    ProjectResponse,
    Workflow,
};

pub struct CircleCIClient {
    client: Arc<reqwest::Client>,
    token: HeaderValue,
    base_url: Url,
}

pub fn validate_project_slug(slug: &str) -> PluginResult<()> {
    let mut parts = slug.split('/');
    let valid = matches!(parts.next(), Some("gh" | "bb" | "circleci"))
        && parts.clone().count() == 2
        && parts.all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        });
    if valid {
        Ok(())
    } else {
        Err(PluginError::InvalidConfig(
            "Use a CircleCI project slug: gh/org/repo, bb/org/repo or circleci/org-id/project-id"
                .into(),
        ))
    }
}

impl CircleCIClient {
    pub fn new(client: Arc<reqwest::Client>, token: String, base_url: &str) -> PluginResult<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|_| PluginError::InvalidConfig("Invalid CircleCI base URL".into()))?;
        if !matches!(base_url.scheme(), "http" | "https")
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || base_url.path() != "/"
        {
            return Err(PluginError::InvalidConfig(
                "CircleCI base URL must be an HTTP or HTTPS origin without credentials".into(),
            ));
        }
        let mut token = HeaderValue::from_str(&token)
            .map_err(|_| PluginError::InvalidConfig("Invalid CircleCI token header".into()))?;
        token.set_sensitive(true);
        Ok(Self {
            client,
            token,
            base_url,
        })
    }

    fn url(&self, segments: &[&str]) -> PluginResult<Url> {
        if segments
            .iter()
            .any(|part| part.is_empty() || matches!(*part, "." | ".."))
        {
            return Err(PluginError::InvalidConfig(
                "Empty or relative CircleCI path component".into(),
            ));
        }
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| PluginError::InvalidConfig("Invalid CircleCI base URL".into()))?
            .clear()
            .extend(segments);
        Ok(url)
    }

    fn project_url(&self, slug: &str, suffix: &[&str]) -> PluginResult<Url> {
        validate_project_slug(slug)?;
        let mut parts = vec!["api", "v2", "project"];
        parts.extend(slug.split('/'));
        parts.extend_from_slice(suffix);
        self.url(&parts)
    }

    async fn request(
        &self, method: Method, url: Url, body: Option<Value>,
    ) -> PluginResult<reqwest::Response> {
        let mut request = self
            .client
            .request(method, url.clone())
            .header("Circle-Token", self.token.clone())
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|err| http::map_send_error(err, &url))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let retry_after = http::retry_after(response.headers(), chrono::Utc::now());
        let exhausted = response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|value| value == "0");
        let code = if status.as_u16() == 403
            && (exhausted || response.headers().contains_key("retry-after"))
        {
            429
        } else {
            status.as_u16()
        };
        Err(http::from_status(
            code,
            format!("CircleCI returned HTTP {}", status.as_u16()),
            retry_after,
        ))
    }

    async fn get<T: DeserializeOwned>(&self, url: Url) -> PluginResult<T> {
        Self::decode(self.request(Method::GET, url, None).await?).await
    }

    async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> PluginResult<T> {
        response
            .json()
            .await
            .map_err(|_| PluginError::SerializationError("Invalid CircleCI JSON response".into()))
    }

    async fn pages<T: DeserializeOwned>(&self, mut url: Url, limit: usize) -> PluginResult<Vec<T>> {
        let mut items = Vec::new();
        let mut tokens = HashSet::new();
        while items.len() < limit {
            let page: Page<T> = self.get(url.clone()).await?;
            items.extend(page.items.into_iter().take(limit - items.len()));
            let Some(token) = page.next_page_token.filter(|token| !token.is_empty()) else {
                break;
            };
            if items.len() == limit {
                break;
            }
            if !tokens.insert(token.clone()) {
                return Err(PluginError::ApiError(
                    "CircleCI repeated a pagination token".into(),
                ));
            }
            url.query_pairs_mut()
                .clear()
                .append_pair("page-token", &token);
        }
        Ok(items)
    }

    pub async fn validate_credentials(&self) -> PluginResult<bool> {
        self.request(Method::GET, self.url(&["api", "v2", "me"])?, None)
            .await?;
        Ok(true)
    }

    pub async fn fetch_projects(&self) -> PluginResult<Vec<Project>> {
        let projects: Vec<FollowedProject> =
            self.get(self.url(&["api", "v1.1", "projects"])?).await?;
        projects
            .into_iter()
            .map(|project| {
                let vcs = match project.vcs_type.as_deref() {
                    Some("github") => "gh",
                    Some("bitbucket") => "bb",
                    None => match Url::parse(&project.vcs_url)
                        .ok()
                        .as_ref()
                        .and_then(Url::host_str)
                    {
                        Some("github.com") => "gh",
                        Some("bitbucket.org") => "bb",
                        _ => return Err(PluginError::ApiError("Unknown CircleCI VCS host".into())),
                    },
                    _ => return Err(PluginError::ApiError("Unknown CircleCI VCS type".into())),
                };
                let slug = format!("{vcs}/{}/{}", project.username, project.reponame);
                validate_project_slug(&slug)?;
                Ok(Project {
                    slug,
                    name: project.reponame,
                    organization: project.username,
                    repository_url: project.vcs_url,
                    default_branch: project.default_branch,
                })
            })
            .collect()
    }

    pub async fn fetch_project(&self, slug: &str) -> PluginResult<Project> {
        let project: ProjectResponse = self.get(self.project_url(slug, &[])?).await?;
        validate_project_slug(&project.slug)?;
        let (repository_url, default_branch) = project
            .vcs_info
            .map(|vcs| (vcs.vcs_url, vcs.default_branch))
            .unwrap_or_default();
        Ok(Project {
            slug: project.slug,
            name: project.name,
            organization: project.organization_name,
            repository_url,
            default_branch,
        })
    }

    pub async fn fetch_pipelines(
        &self, slug: &str, limit: usize,
    ) -> PluginResult<Vec<CirclePipeline>> {
        self.pages(self.project_url(slug, &["pipeline"])?, limit)
            .await
    }

    pub async fn fetch_pipeline(&self, slug: &str, number: i64) -> PluginResult<CirclePipeline> {
        self.get(self.project_url(slug, &["pipeline", &number.to_string()])?)
            .await
    }

    pub async fn fetch_workflows(&self, pipeline_id: &str) -> PluginResult<Vec<Workflow>> {
        self.pages(
            self.url(&["api", "v2", "pipeline", pipeline_id, "workflow"])?,
            usize::MAX,
        )
        .await
    }

    pub async fn trigger(&self, slug: &str, body: Value) -> PluginResult<String> {
        let url = self.project_url(slug, &["pipeline"])?;
        if slug.starts_with("circleci/") {
            return Err(PluginError::NotSupported(
                "CircleCI App projects require pipeline definitions. Trigger them in CircleCI"
                    .into(),
            ));
        }
        let response: Value =
            Self::decode(self.request(Method::POST, url, Some(body)).await?).await?;
        response
            .get("number")
            .and_then(Value::as_i64)
            .filter(|number| *number > 0)
            .map(|number| number.to_string())
            .ok_or_else(|| {
                PluginError::ApiError("CircleCI trigger response has no pipeline number".into())
            })
    }

    pub async fn cancel(&self, workflow_id: &str) -> PluginResult<()> {
        self.request(
            Method::POST,
            self.url(&["api", "v2", "workflow", workflow_id, "cancel"])?,
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn rerun(&self, workflow_id: &str, failed_only: bool) -> PluginResult<()> {
        self.request(
            Method::POST,
            self.url(&["api", "v2", "workflow", workflow_id, "rerun"])?,
            Some(json!({"from_failed": failed_only})),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{
        Read,
        Write,
    };
    use std::net::TcpListener;
    use std::thread::{
        self,
        JoinHandle,
    };
    use std::time::Duration;

    use super::*;

    fn server(responses: Vec<(u16, Value)>) -> (CircleCIClient, JoinHandle<Vec<String>>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .filter_map(|line| line.split_once(':'))
                            .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                            .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                let body = body.to_string();
                let retry = if matches!(status, 429 | 503) {
                    "Retry-After: 12\r\n"
                } else {
                    ""
                };
                write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        let client = CircleCIClient::new(
            Arc::new(reqwest::Client::new()),
            "test-token".into(),
            &origin,
        )
        .unwrap();
        (client, handle)
    }

    fn pipeline(number: i64) -> Value {
        json!({"id": format!("id-{number}"), "number": number, "state": "created",
            "created_at": "2026-01-01T00:00:00Z"})
    }

    #[tokio::test]
    async fn pipeline_pages_preserve_order_encode_cursors_and_stop_at_limit() {
        let (client, handle) = server(vec![
            (
                200,
                json!({"items": [pipeline(4)], "next_page_token": "a+b&c"}),
            ),
            (
                200,
                json!({"items": [pipeline(3), pipeline(2)], "next_page_token": "unused"}),
            ),
        ]);
        assert!(client
            .fetch_pipelines("gh/org/repo", 0)
            .await
            .unwrap()
            .is_empty());
        let pipelines = client.fetch_pipelines("gh/org/repo", 2).await.unwrap();
        assert_eq!(
            pipelines
                .iter()
                .map(|pipeline| pipeline.number)
                .collect::<Vec<_>>(),
            [4, 3]
        );
        let requests = handle.join().unwrap();
        assert!(requests[0].starts_with("GET /api/v2/project/gh/org/repo/pipeline HTTP"));
        assert!(requests[1].contains("?page-token=a%2Bb%26c "));
        assert!(requests[0]
            .to_lowercase()
            .contains("circle-token: test-token\r\n"));
        assert!(requests[0]
            .to_lowercase()
            .contains("accept: application/json\r\n"));
        assert!(!requests[0].lines().next().unwrap().contains("test-token"));
    }

    #[tokio::test]
    async fn workflows_follow_all_pages_and_encode_ids() {
        let (client, handle) = server(vec![
            (
                200,
                json!({"items": [{"id": "one", "name": "build", "status": "success"}], "next_page_token": "next"}),
            ),
            (
                200,
                json!({"items": [{"id": "two", "name": "test", "status": "failed"}], "next_page_token": null}),
            ),
        ]);
        let workflows = client.fetch_workflows("id/with?query").await.unwrap();
        assert_eq!(workflows.len(), 2);
        assert_eq!(workflows[1].status, "failed");
        let requests = handle.join().unwrap();
        assert!(requests[0].starts_with("GET /api/v2/pipeline/id%2Fwith%3Fquery/workflow "));
        assert!(requests[1].contains("?page-token=next "));
    }

    #[tokio::test]
    async fn pagination_rejects_repeated_tokens_and_propagates_later_errors() {
        let page = json!({"items": [], "next_page_token": "same"});
        let (client, handle) = server(vec![(200, page.clone()), (200, page)]);
        assert!(matches!(
            client.fetch_workflows("id").await,
            Err(PluginError::ApiError(_))
        ));
        handle.join().unwrap();
        let (client, handle) = server(vec![
            (
                200,
                json!({"items": [pipeline(1)], "next_page_token": "next"}),
            ),
            (503, json!({"message": "unavailable"})),
        ]);
        assert!(matches!(
            client.fetch_pipelines("gh/org/repo", 2).await,
            Err(PluginError::ServiceUnavailable { .. })
        ));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn triggers_send_branch_and_typed_parameters_and_return_pipeline_number() {
        let (client, handle) = server(vec![
            (201, json!({"number": 42})),
            (201, json!({"number": 43})),
        ]);
        let body = json!({"branch": "release/next", "parameters": {"deploy": true, "count": 3, "name": "ship"}});
        assert_eq!(
            client.trigger("gh/org/repo", body.clone()).await.unwrap(),
            "42"
        );
        assert_eq!(
            client.trigger("bb/org/repo", json!({})).await.unwrap(),
            "43"
        );
        assert!(matches!(
            client.trigger("circleci/org/project", body.clone()).await,
            Err(PluginError::NotSupported(_))
        ));
        let requests = handle.join().unwrap();
        assert!(requests[0].starts_with("POST /api/v2/project/gh/org/repo/pipeline "));
        assert!(requests[1].starts_with("POST /api/v2/project/bb/org/repo/pipeline "));
        assert_eq!(
            serde_json::from_str::<Value>(requests[0].split_once("\r\n\r\n").unwrap().1).unwrap(),
            body
        );
    }

    #[tokio::test]
    async fn actions_reject_failures_and_missing_trigger_numbers() {
        let (client, handle) = server(vec![
            (201, json!({"id": "uuid"})),
            (404, json!({"message": "missing"})),
            (202, json!({"message": "canceled"})),
            (202, json!({"workflow_id": "new"})),
        ]);
        assert!(matches!(
            client.trigger("gh/org/repo", json!({})).await,
            Err(PluginError::ApiError(_))
        ));
        assert!(matches!(
            client.cancel("missing").await,
            Err(PluginError::PipelineNotFound(_))
        ));
        client.cancel("workflow/id").await.unwrap();
        client.rerun("workflow/id", true).await.unwrap();
        let requests = handle.join().unwrap();
        assert!(requests[2].starts_with("POST /api/v2/workflow/workflow%2Fid/cancel "));
        assert!(requests[3].starts_with("POST /api/v2/workflow/workflow%2Fid/rerun "));
        assert_eq!(
            serde_json::from_str::<Value>(requests[3].split_once("\r\n\r\n").unwrap().1).unwrap(),
            json!({"from_failed": true})
        );
    }

    #[tokio::test]
    async fn shared_http_errors_preserve_retry_delays_and_hide_response_secrets() {
        for status in [401, 403, 404, 429, 503, 302, 400] {
            let (client, handle) = server(vec![(status, json!({"message": "test-token"}))]);
            let error = client.validate_credentials().await.unwrap_err();
            match status {
                401 => assert!(matches!(error, PluginError::AuthenticationFailed(_))),
                403 => assert!(matches!(error, PluginError::PermissionDenied(_))),
                404 => assert!(matches!(error, PluginError::PipelineNotFound(_))),
                429 => assert!(matches!(error, PluginError::RateLimited { .. })),
                503 => assert!(matches!(error, PluginError::ServiceUnavailable { .. })),
                _ => assert!(matches!(error, PluginError::ApiError(_))),
            }
            if matches!(status, 429 | 503) {
                assert_eq!(error.retry_after(), Some(Duration::from_secs(12)));
            }
            assert!(!error.to_string().contains("test-token"));
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn discovers_followed_projects_and_fetches_app_projects_and_details() {
        let (client, handle) = server(vec![
            (
                200,
                json!([{"vcs_type": "github", "username": "org", "reponame": "repo",
                "vcs_url": "https://github.com/org/repo", "default_branch": "main"}]),
            ),
            (
                200,
                json!({"slug": "circleci/org/project", "name": "app", "organization_name": "org",
                "vcs_info": {"vcs_url": "https://github.com/org/app", "default_branch": "develop"}}),
            ),
            (200, pipeline(42)),
            (200, json!({"items": "invalid"})),
        ]);
        let projects = client.fetch_projects().await.unwrap();
        assert_eq!(projects[0].slug, "gh/org/repo");
        assert_eq!(projects[0].default_branch.as_deref(), Some("main"));
        let project = client.fetch_project("circleci/org/project").await.unwrap();
        assert_eq!(project.name, "app");
        assert_eq!(project.default_branch.as_deref(), Some("develop"));
        assert_eq!(
            client
                .fetch_pipeline("circleci/org/project", 42)
                .await
                .unwrap()
                .number,
            42
        );
        assert!(matches!(
            client.fetch_pipelines("gh/org/repo", 1).await,
            Err(PluginError::SerializationError(_))
        ));
        let requests = handle.join().unwrap();
        assert!(requests[0].starts_with("GET /api/v1.1/projects "));
        assert!(requests[2].starts_with("GET /api/v2/project/circleci/org/project/pipeline/42 "));
    }

    #[tokio::test]
    async fn discovers_projects_when_vcs_type_is_omitted() {
        let (client, handle) = server(vec![(
            200,
            json!([
                {"username": "org", "reponame": "repo", "vcs_url": "https://github.com/org/repo"},
                {"username": "org", "reponame": "app", "vcs_url": "https://bitbucket.org/org/app"}
            ]),
        )]);
        let projects = client.fetch_projects().await.unwrap();
        assert_eq!(projects[0].slug, "gh/org/repo");
        assert_eq!(projects[1].slug, "bb/org/app");
        handle.join().unwrap();
    }

    #[test]
    fn rejects_unsafe_origins_and_project_paths() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        for origin in [
            "ftp://circleci.com",
            "https://user:token@circleci.com",
            "https://circleci.com/api",
            "https://circleci.com?token=x",
        ] {
            assert!(
                CircleCIClient::new(Arc::new(reqwest::Client::new()), "token".into(), origin)
                    .is_err()
            );
        }
        for slug in [
            "gh/org",
            "gh/org/repo/extra",
            "gh/../repo",
            "gh/org/repo?x",
            "gh/org/%2e%2e",
            "other/org/repo",
        ] {
            assert!(validate_project_slug(slug).is_err(), "{slug}");
        }
        for slug in ["gh/org/repo", "bb/org/repo", "circleci/org-id/project-id"] {
            validate_project_slug(slug).unwrap();
        }
    }
}
