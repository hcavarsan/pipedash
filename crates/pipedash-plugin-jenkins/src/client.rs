use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::http::{
    decode_json,
    parse_http_url,
    send,
    send_checked,
};
use pipedash_plugin_api::{
    check_response,
    AvailablePipeline,
    Pipeline,
    PluginResult,
    RetryPolicy,
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

mod logs;

pub(crate) struct JenkinsClient {
    http_client: std::sync::Arc<Client>,
    server_url: String,
    auth_header: String,
    pub(crate) retry_policy: RetryPolicy,
}

impl JenkinsClient {
    pub fn new(
        http_client: std::sync::Arc<Client>, server_url: String, auth_header: String,
    ) -> Self {
        Self {
            http_client,
            server_url,
            auth_header,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    fn authorized(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .header(reqwest::header::AUTHORIZATION, &self.auth_header)
    }

    async fn get_json<T: DeserializeOwned>(
        &self, url: &str, timeout: Option<Duration>,
    ) -> PluginResult<T> {
        let url = parse_http_url(url, "Jenkins")?;

        self.retry_policy
            .retry(|| async {
                let mut request = self.authorized(Method::GET, &url);
                if let Some(timeout) = timeout {
                    request = request.timeout(timeout);
                }
                let response = send(&url, request).await?;

                decode_json(check_response(response).await?, "Jenkins response").await
            })
            .await
    }

    pub async fn discover_all_jobs(&self) -> PluginResult<Vec<types::DiscoveredJob>> {
        let mut all_jobs = Vec::new();
        let mut queue = vec![String::new()];

        while let Some(path) = queue.pop() {
            let jobs = self.fetch_jobs_in_folder(&path).await?;

            for job in jobs {
                let full_path = if path.is_empty() {
                    job.name.clone()
                } else {
                    format!("{}/{}", path, job.name)
                };

                if job._class.contains("Folder")
                    || job._class.contains("WorkflowMultiBranchProject")
                {
                    queue.push(full_path);
                } else if job._class.contains("WorkflowJob")
                    || job._class.contains("FreeStyleProject")
                {
                    all_jobs.push(types::DiscoveredJob {
                        name: job.name,
                        full_path,
                        _class: job._class,
                    });
                }
            }
        }

        Ok(all_jobs)
    }

    async fn fetch_jobs_in_folder(&self, folder_path: &str) -> PluginResult<Vec<types::JobItem>> {
        let url = if folder_path.is_empty() {
            format!("{}/api/json?tree=jobs[name,url,_class]", self.server_url)
        } else {
            let encoded_path = config::encode_job_name(folder_path);
            format!(
                "{}/job/{}/api/json?tree=jobs[name,url,_class]",
                self.server_url, encoded_path
            )
        };

        let response: types::JobsResponse = self.get_json(&url, None).await?;

        Ok(response.jobs)
    }

    pub async fn fetch_job_details(&self, job_path: &str) -> PluginResult<types::Job> {
        let encoded_path = config::encode_job_name(job_path);
        let url = format!(
            "{}/job/{}/api/json?tree=name,lastBuild[number]",
            self.server_url, encoded_path
        );

        self.get_json(&url, Some(Duration::from_secs(10))).await
    }

    pub async fn fetch_build_details(
        &self, job_path: &str, build_number: i64,
    ) -> PluginResult<types::Build> {
        let encoded_path = config::encode_job_name(job_path);
        let url = format!(
            "{}/job/{}/{}/api/json?tree=number,result,building,timestamp,duration,url,fullDisplayName,actions[_class,causes[userName,shortDescription],lastBuiltRevision[SHA1,branch[SHA1,name]],parameters[name,value]]",
            self.server_url, encoded_path, build_number
        );

        tracing::debug!(job_path = %job_path, build_number = build_number, "Fetching Jenkins build details");

        let build: types::Build = self.get_json(&url, Some(Duration::from_secs(10))).await?;

        tracing::debug!(action_count = build.actions.len(), "Parsed Jenkins build");

        Ok(build)
    }

    pub async fn fetch_build_history(
        &self, job_path: &str, limit: usize,
    ) -> PluginResult<Vec<types::Build>> {
        let encoded_path = config::encode_job_name(job_path);
        let url = format!(
            "{}/job/{}/api/json?tree=builds[number,url,result,building,timestamp,duration,actions[_class,causes[userName,shortDescription],lastBuiltRevision[SHA1,branch[SHA1,name]],parameters[name,value]]]{{0,{limit}}}",
            self.server_url, encoded_path
        );

        let response: types::JobBuildsResponse = self.get_json(&url, None).await?;

        Ok(response.builds)
    }

    pub async fn fetch_job_parameters(
        &self, job_path: &str,
    ) -> PluginResult<types::JobWithParameters> {
        let encoded_path = config::encode_job_name(job_path);
        let url = format!(
            "{}/job/{}/api/json?tree=property[parameterDefinitions[name,description,type,defaultParameterValue[value],choices],_class]",
            self.server_url, encoded_path
        );

        tracing::debug!(job_path = %job_path, "Fetching Jenkins job parameters");

        self.get_json(&url, Some(Duration::from_secs(30))).await
    }

    pub async fn trigger_build(
        &self, job_path: &str, form_data: Vec<(String, String)>,
    ) -> PluginResult<()> {
        let encoded_path = config::encode_job_name(job_path);
        let endpoint = if form_data.is_empty() {
            "build"
        } else {
            "buildWithParameters"
        };
        let url = parse_http_url(
            &format!("{}/job/{}/{endpoint}", self.server_url, encoded_path),
            "Jenkins",
        )?;
        let parameter_names = form_data
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        tracing::debug!(job_path = %job_path, parameters = %parameter_names, "Triggering Jenkins build");

        send_checked(&self.retry_policy, &url, || {
            self.authorized(Method::POST, &url).form(&form_data)
        })
        .await
        .inspect_err(|e| {
            tracing::error!(job_path = %job_path, parameters = %parameter_names, error = %e, "Jenkins trigger failed");
        })?;

        tracing::info!("Jenkins build triggered successfully");
        Ok(())
    }

    pub async fn fetch_pipeline(&self, job_path: String) -> PluginResult<Pipeline> {
        let pipeline_start = std::time::Instant::now();
        tracing::debug!(job_path = %job_path, "Fetching Jenkins pipeline");

        let job_start = std::time::Instant::now();
        let job = self.fetch_job_details(&job_path).await?;
        tracing::debug!(elapsed = ?job_start.elapsed(), "Fetched Jenkins job details");

        let (status, last_run) = if let Some(ref last_build) = job.last_build {
            let build_start = std::time::Instant::now();
            let build = self
                .fetch_build_details(&job_path, last_build.number)
                .await?;
            tracing::debug!(elapsed = ?build_start.elapsed(), "Fetched Jenkins build details");

            let build_status = mapper::build_status(&build);
            let build_time = chrono::DateTime::from_timestamp_millis(build.timestamp)
                .map(|dt| dt.with_timezone(&Utc));
            (build_status, build_time)
        } else {
            tracing::debug!("No last Jenkins build found");
            (pipedash_plugin_api::PipelineStatus::Pending, None)
        };

        let (org, repo) = config::split_job_path(&job_path);
        let repository_field = if job_path.contains('/') {
            job_path.clone()
        } else {
            format!("{org}/{repo}")
        };

        tracing::debug!(elapsed = ?pipeline_start.elapsed(), "Total Jenkins pipeline fetch time");

        Ok(Pipeline {
            id: job_path,
            name: job.name,
            status,
            last_run,
            last_updated: Utc::now(),
            repository: repository_field,
            branch: None,
            workflow_file: None,
            resource_state: None,
            metadata: HashMap::new(),
        })
    }

    pub async fn cancel_build(&self, job_path: &str, build_number: i64) -> PluginResult<()> {
        let encoded_path = config::encode_job_name(job_path);
        let url = parse_http_url(
            &format!(
                "{}/job/{}/{}/stop",
                self.server_url, encoded_path, build_number
            ),
            "Jenkins",
        )?;

        tracing::info!(build_number = build_number, job_path = %job_path, "Cancelling Jenkins build");

        send_checked(&self.retry_policy, &url, || {
            self.authorized(Method::POST, &url)
        })
        .await?;

        tracing::info!(
            build_number = build_number,
            "Jenkins build cancelled successfully"
        );
        Ok(())
    }

    pub fn discovered_jobs_to_available_pipelines(
        &self, all_jobs: Vec<types::DiscoveredJob>,
    ) -> Vec<AvailablePipeline> {
        all_jobs
            .into_iter()
            .map(|job| {
                let job_type = if job._class.contains("WorkflowJob") {
                    "Pipeline"
                } else if job._class.contains("FreeStyleProject") {
                    "Freestyle"
                } else {
                    "Job"
                };

                let (organization, repository) = config::split_job_path(&job.full_path);

                AvailablePipeline {
                    id: job.full_path.clone(),
                    name: job.name,
                    description: Some(format!("Type: {job_type}")),
                    organization: Some(organization),
                    repository: Some(repository),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
