use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::http::{
    decode_json,
    parse_http_url,
};
use pipedash_plugin_api::utils::{
    parse_timestamp,
    repository_name,
};
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    AvailablePipeline,
    Pipeline,
    PluginResult,
    RetryPolicy,
};
use reqwest::Client;
use serde::de::DeserializeOwned;

use crate::{
    mapper,
    types,
};

pub(crate) mod catalog;
mod logs;

const BASE_URL: &str = "https://api.buildkite.com/v2";

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
        let url = parse_http_url(url, "Buildkite")?;

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

                decode_json(check_response(response).await?, "Buildkite response").await
            })
            .await
    }

    pub async fn fetch_organizations(&self) -> PluginResult<Vec<types::Organization>> {
        let mut organizations = Vec::new();
        let mut page = 1;
        loop {
            let batch: Vec<types::Organization> = self
                .get_json(&format!(
                    "{}/organizations?per_page=100&page={page}",
                    self.base_url
                ))
                .await?;
            let is_last_page = batch.len() < 100;
            organizations.extend(batch);
            if is_last_page {
                break;
            }
            page += 1;
        }
        Ok(organizations)
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
                let repo_name = repository_name(&pipeline.repository);
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

    pub async fn fetch_pipeline(&self, org: String, slug: String) -> PluginResult<Pipeline> {
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

        let last_run = latest_build.and_then(|build| parse_timestamp(&build.created_at));

        let mut metadata = HashMap::new();
        metadata.insert(
            "organization_slug".to_string(),
            serde_json::json!(org.clone()),
        );

        Ok(Pipeline {
            id: format!("{org}__{slug}"),
            name: pipeline.name,
            status,
            last_run,
            last_updated: Utc::now(),
            repository: repository_name(&pipeline.repository),
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
        let total_pages = limit.div_ceil(per_page);
        let mut all_builds = Vec::new();

        for page in 1..=total_pages {
            let builds: Vec<types::Build> = self
                .get_json(&format!(
                    "{}/organizations/{org}/pipelines/{slug}/builds?per_page={}&page={}",
                    self.base_url, per_page, page
                ))
                .await?;
            let is_last_page = builds.len() < per_page;
            all_builds.extend(builds);
            if is_last_page {
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

    pub async fn trigger_build(
        &self, org: &str, slug: &str, branch: String, inputs: Option<serde_json::Value>,
    ) -> PluginResult<types::Build> {
        let url = parse_http_url(
            &format!(
                "{}/organizations/{org}/pipelines/{slug}/builds",
                self.base_url
            ),
            "Buildkite",
        )?;

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

        decode_json(check_response(response).await?, "Buildkite build").await
    }

    pub async fn cancel_build(
        &self, org: &str, pipeline_slug: &str, build_number: i64,
    ) -> PluginResult<()> {
        let url = parse_http_url(
            &format!(
                "{}/organizations/{org}/pipelines/{pipeline_slug}/builds/{build_number}/cancel",
                self.base_url
            ),
            "Buildkite",
        )?;

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

#[cfg(test)]
mod tests;
