use pipedash_plugin_api::{
    http,
    PluginError,
    PluginResult,
};
use reqwest::Url;

use super::{
    error_for,
    validate_project_slug,
    CircleCIClient,
};
use crate::types::{
    JobSteps,
    OutputMessage,
};

impl CircleCIClient {
    pub async fn fetch_job_steps(&self, slug: &str, job_number: i64) -> PluginResult<JobSteps> {
        validate_project_slug(slug)?;
        let mut parts = slug.split('/');
        let vcs = match parts.next() {
            Some("gh") => "github",
            Some("bb") => "bitbucket",
            _ => "circleci",
        };
        let number = job_number.to_string();
        let mut segments = vec!["api", "v1.1", "project", vcs];
        segments.extend(parts);
        segments.push(&number);
        self.get(self.url(&segments)?).await
    }

    fn output_url(&self, output_url: &str) -> PluginResult<Url> {
        Url::parse(output_url)
            .ok()
            .filter(|url| {
                url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && (url.scheme() == "https" || url.origin() == self.base_url.origin())
            })
            .ok_or_else(|| PluginError::ApiError("CircleCI sent an invalid step output URL".into()))
    }

    pub async fn fetch_step_output(
        &self, output_url: &str, limit: usize,
    ) -> PluginResult<Option<Vec<OutputMessage>>> {
        let url = self.output_url(output_url)?;
        let mut shown = url.clone();
        shown.set_query(None);
        self.retry_policy
            .retry(|| async {
                let response = self
                    .client
                    .get(url.clone())
                    .send()
                    .await
                    .map_err(|err| http::map_send_error(err.without_url(), &shown))?;
                if !response.status().is_success() {
                    return Err(error_for(&response, "CircleCI step output"));
                }
                let body = http::read_prefix(response, limit.saturating_add(1))
                    .await
                    .map_err(|err| http::map_send_error(err.without_url(), &shown))?;
                if body.len() > limit {
                    return Ok(None);
                }
                serde_json::from_slice(&body).map(Some).map_err(|_| {
                    PluginError::SerializationError("Invalid CircleCI step output".into())
                })
            })
            .await
    }
}
