use pipedash_plugin_api::http::decode_json;
use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    LogReadMode,
    LogSink,
    PluginResult,
};

use super::GitLabClient;
use crate::types::Job;

impl GitLabClient {
    pub async fn get_pipeline_jobs(
        &self, project_id: i64, pipeline_id: i64,
    ) -> PluginResult<Vec<Job>> {
        let mut jobs = Vec::new();
        let mut page = 1;
        loop {
            let (batch, next) = self
                .retry_policy
                .retry(|| async {
                    let response = self
                        .get(&format!(
                            "{}/projects/{}/pipelines/{}/jobs?per_page=100&include_retried=true&page={}",
                            self.api_url, project_id, pipeline_id, page
                        ))
                        .await?;
                    let next = Self::next_page(&response);
                    Ok((
                        decode_json::<Vec<Job>>(response, "GitLab API response").await?,
                        next,
                    ))
                })
                .await?;
            jobs.extend(batch);
            match next {
                Some(next) if next > page => page = next,
                _ => return Ok(jobs),
            }
        }
    }

    pub async fn stream_job_log(
        &self, project_id: i64, job_id: &str, offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let url = Self::parse_url(&format!(
            "{}/projects/{}/jobs/{}/trace",
            self.api_url, project_id, job_id
        ))?;

        let url = &url;
        let gone = format!("Logs for job {job_id} are no longer available");
        sink.pipe_resumed(offset, |from| {
            send_log(&self.retry_policy, url, from, &gone, || {
                self.http_client
                    .get(url.clone())
                    .header("PRIVATE-TOKEN", &self.token)
                    .header(reqwest::header::ACCEPT, "text/plain")
            })
        })
        .await
    }
}
