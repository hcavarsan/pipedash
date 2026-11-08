use pipedash_plugin_api::http::parse_http_url;
use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    check_response,
    LogSink,
    PluginError,
    PluginResult,
};
use reqwest::header::ACCEPT;
use secrecy::ExposeSecret;

use super::{
    map_octocrab_error,
    GitHubClient,
};
use crate::types;

const RUN_HISTORY_PAGE_SIZE: usize = 100;

impl GitHubClient {
    async fn fetch_run_page(&self, route: &str, page: usize) -> PluginResult<Vec<types::Run>> {
        let response: octocrab::Page<types::Run> = self
            .retry_policy
            .retry(|| async {
                self.octocrab
                    .get(
                        route,
                        Some(&[("per_page", RUN_HISTORY_PAGE_SIZE), ("page", page)]),
                    )
                    .await
                    .map_err(|e| {
                        map_octocrab_error(&format!("Failed to fetch run history page {page}"), e)
                    })
            })
            .await?;

        Ok(response.items)
    }

    pub async fn fetch_run_history(
        &self, owner: &str, repo: &str, workflow_id: u64, limit: usize,
    ) -> PluginResult<Vec<types::Run>> {
        let route = format!("/repos/{owner}/{repo}/actions/workflows/{workflow_id}/runs");
        let mut all_runs = Vec::new();
        let mut page = 1usize;

        while all_runs.len() < limit {
            let runs = self.fetch_run_page(&route, page).await?;
            let short_page = runs.len() < RUN_HISTORY_PAGE_SIZE;
            all_runs.extend(runs.into_iter().take(limit - all_runs.len()));
            if short_page {
                break;
            }
            page += 1;
        }

        Ok(all_runs)
    }

    pub async fn fetch_run_by_number(
        &self, owner: &str, repo: &str, workflow_id: u64, run_number: i64,
    ) -> PluginResult<types::Run> {
        let route = format!("/repos/{owner}/{repo}/actions/workflows/{workflow_id}/runs");
        let mut page = 1usize;

        loop {
            let runs = self.fetch_run_page(&route, page).await?;
            let short_page = runs.len() < RUN_HISTORY_PAGE_SIZE;
            if let Some(run) = runs.into_iter().find(|run| run.run_number == run_number) {
                return Ok(run);
            }
            if short_page {
                break;
            }
            page += 1;
        }

        Err(PluginError::PipelineNotFound(format!(
            "Run #{run_number} not found"
        )))
    }

    pub async fn list_run_jobs(
        &self, owner: &str, repo: &str, run_id: u64,
    ) -> PluginResult<Vec<octocrab::models::workflows::Job>> {
        const PAGE_SIZE: usize = 100;

        let mut jobs = Vec::new();

        for page in 1u32..=100 {
            let batch = self
                .retry_policy
                .retry(|| async {
                    self.octocrab
                        .workflows(owner, repo)
                        .list_jobs(octocrab::models::RunId(run_id))
                        .filter(octocrab::params::workflows::Filter::Latest)
                        .per_page(PAGE_SIZE as u8)
                        .page(page)
                        .send()
                        .await
                        .map_err(|e| map_octocrab_error("Failed to list jobs", e))
                })
                .await?;

            let short_page = batch.items.len() < PAGE_SIZE;
            let total_count = batch.total_count;
            jobs.extend(batch.items);

            if short_page || total_count.is_some_and(|total| jobs.len() as u64 >= total) {
                return Ok(jobs);
            }
        }

        Err(PluginError::ApiError("Run has more than 10000 jobs".into()))
    }

    pub async fn stream_job_log(
        &self, owner: &str, repo: &str, job_id: u64, sink: &mut LogSink,
    ) -> PluginResult<()> {
        let url = parse_http_url(
            &format!(
                "{}/repos/{owner}/{repo}/actions/jobs/{job_id}/logs",
                self.api_url
            ),
            "log",
        )?;

        let gone = format!("Logs for job {job_id} are no longer available");
        let response = send_log(&self.retry_policy, &url, None, &gone, || {
            self.http
                .get(url.clone())
                .bearer_auth(self.token.expose_secret())
                .header(ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
        })
        .await?;

        sink.pipe(check_response(response).await?).await
    }
}
