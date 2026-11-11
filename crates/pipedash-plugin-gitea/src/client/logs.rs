use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    LogReadMode,
    LogSink,
    PluginError,
    PluginResult,
};
use reqwest::Method;

use super::GiteaClient;
use crate::types::{
    Job,
    JobAttempt,
    JobList,
};

const JOBS_PAGE_SIZE: usize = 50;
const MIN_RUN_LOGS_VERSION: &str = "1.25";

impl GiteaClient {
    pub async fn list_run_jobs(
        &self, owner: &str, repo: &str, run_id: i64,
    ) -> PluginResult<Vec<Job>> {
        let path = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/jobs");
        let mut jobs = Vec::new();
        for page in 1.. {
            let list: JobList = self
                .get(
                    &path,
                    &[
                        ("page", page.to_string()),
                        ("limit", JOBS_PAGE_SIZE.to_string()),
                    ],
                )
                .await
                .map_err(|error| match error {
                    PluginError::PipelineNotFound(_) => PluginError::NotSupported(format!(
                        "Gitea answered 404 for the jobs of run {run_id}. Run logs need Gitea {MIN_RUN_LOGS_VERSION} or newer"
                    )),
                    error => error,
                })?;
            if list.jobs.is_empty() {
                break;
            }
            jobs.extend(list.jobs);
            if list.total_count.is_some_and(|total| jobs.len() >= total) {
                break;
            }
        }
        Ok(jobs)
    }

    pub async fn job_attempt(
        &self, owner: &str, repo: &str, job_id: &str,
    ) -> PluginResult<JobAttempt> {
        let job: Job = self
            .get(&format!("/repos/{owner}/{repo}/actions/jobs/{job_id}"), &[])
            .await?;
        Ok(job.attempt())
    }

    pub async fn stream_job_log(
        &self, owner: &str, repo: &str, job_id: &str, offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let path = format!("/repos/{owner}/{repo}/actions/jobs/{job_id}/logs");
        let url = self.url(&path, &[])?;

        let url = &url;
        let gone = format!("Logs for job {job_id} are no longer available");
        sink.pipe_resumed(offset, |from| {
            send_log(&self.retry_policy, url, from, &gone, || {
                self.request(Method::GET, url)
            })
        })
        .await
    }
}
