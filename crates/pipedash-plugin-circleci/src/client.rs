use std::collections::HashSet;
use std::sync::Arc;

use pipedash_plugin_api::{
    http,
    PluginError,
    PluginResult,
    RetryPolicy,
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
    Job,
    Page,
    PipelineConfig,
    PipelineCreation,
    PipelineDefinition,
    Project,
    ProjectResponse,
    Workflow,
};

mod logs;

pub struct CircleCIClient {
    client: Arc<reqwest::Client>,
    token: HeaderValue,
    base_url: Url,
    retry_policy: RetryPolicy,
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

fn error_for(response: &reqwest::Response, what: &str) -> PluginError {
    let status = response.status().as_u16();
    http::from_response_parts(
        status,
        response.headers(),
        format!("{what} returned HTTP {status}"),
        chrono::Utc::now(),
    )
}

fn created_number(creation: PipelineCreation) -> PluginResult<i64> {
    if let Some(number) = creation.number.filter(|number| *number > 0) {
        return Ok(number);
    }
    Err(PluginError::ApiError(match creation.message {
        Some(message) if !message.is_empty() => {
            format!("CircleCI did not start a pipeline: {message}")
        }
        _ => "CircleCI trigger response has no pipeline number".into(),
    }))
}

impl CircleCIClient {
    pub fn new(client: Arc<reqwest::Client>, token: String, base_url: &str) -> PluginResult<Self> {
        let base_url = http::parse_http_url(base_url, "CircleCI base")?;
        if !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || base_url.path() != "/"
        {
            return Err(PluginError::InvalidConfig(
                "CircleCI base URL must be an HTTP or HTTPS origin without credentials".into(),
            ));
        }
        let token = http::sensitive_header(&token, "CircleCI token")?;
        Ok(Self {
            client,
            token,
            base_url,
            retry_policy: RetryPolicy::default(),
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
        &self, method: Method, url: Url, body: Option<&Value>,
    ) -> PluginResult<reqwest::Response> {
        let mut request = self
            .client
            .request(method, url.clone())
            .header("Circle-Token", self.token.clone())
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|err| http::map_send_error(err, &url))?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(error_for(&response, "CircleCI"))
        }
    }

    async fn get<T: DeserializeOwned>(&self, url: Url) -> PluginResult<T> {
        self.retry_policy
            .retry(|| async {
                let response = self.request(Method::GET, url.clone(), None).await?;
                http::decode_json(response, "CircleCI response").await
            })
            .await
    }

    async fn post<T: DeserializeOwned>(&self, url: Url, body: Option<&Value>) -> PluginResult<T> {
        let response = self.request(Method::POST, url, body).await?;
        http::decode_json(response, "CircleCI response").await
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
                    id: None,
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
            id: project.id,
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

    pub async fn fetch_jobs(&self, workflow_id: &str) -> PluginResult<Vec<Job>> {
        self.pages(
            self.url(&["api", "v2", "workflow", workflow_id, "job"])?,
            usize::MAX,
        )
        .await
    }

    pub async fn fetch_pipeline_config(&self, pipeline_id: &str) -> PluginResult<Option<String>> {
        let config: PipelineConfig = self
            .get(self.url(&["api", "v2", "pipeline", pipeline_id, "config"])?)
            .await?;
        Ok(config.source)
    }

    pub async fn fetch_definitions(
        &self, project_id: &str,
    ) -> PluginResult<Vec<PipelineDefinition>> {
        self.pages(
            self.url(&["api", "v2", "projects", project_id, "pipeline-definitions"])?,
            usize::MAX,
        )
        .await
    }

    pub async fn trigger(&self, slug: &str, body: &Value) -> PluginResult<i64> {
        if slug.starts_with("circleci/") {
            return Err(PluginError::NotSupported(
                "CircleCI App projects require a pipeline definition".into(),
            ));
        }
        let url = self.project_url(slug, &["pipeline"])?;
        created_number(self.post(url, Some(body)).await?)
    }

    pub async fn run_pipeline(&self, slug: &str, body: &Value) -> PluginResult<i64> {
        let url = self.project_url(slug, &["pipeline", "run"])?;
        created_number(self.post(url, Some(body)).await?)
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
            Some(&json!({"from_failed": failed_only})),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
