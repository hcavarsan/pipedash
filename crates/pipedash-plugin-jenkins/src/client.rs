use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::Client;

use crate::{
    config,
    mapper,
    types,
};

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

                if job._class.contains("Folder") {
                    queue.push(full_path);
                } else if job._class.contains("WorkflowJob")
                    || job._class.contains("FreeStyleProject")
                    || job._class.contains("WorkflowMultiBranchProject")
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

        let response: types::JobsResponse = self
            .http_client
            .get(&url)
            .header(reqwest::header::AUTHORIZATION, &self.auth_header)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch jobs: {e}")))?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse jobs: {e}")))?;

        Ok(response.jobs)
    }

    pub async fn fetch_job_details(&self, job_path: &str) -> PluginResult<types::Job> {
        let job_path = job_path.to_string();
        self.retry_policy
            .retry(|| async {
                let encoded_path = config::encode_job_name(&job_path);
                let url = format!(
                    "{}/job/{}/api/json?tree=name,lastBuild[number]",
                    self.server_url, encoded_path
                );

                let job: types::Job = self
                    .http_client
                    .get(&url)
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::ApiError(format!("Failed to fetch job {job_path}: {e}"))
                    })?
                    .json()
                    .await
                    .map_err(|e| {
                        PluginError::ApiError(format!("Failed to parse job {job_path}: {e}"))
                    })?;

                Ok(job)
            })
            .await
    }

    pub async fn fetch_build_details(
        &self, job_path: &str, build_number: i64,
    ) -> PluginResult<types::Build> {
        let job_path = job_path.to_string();
        self.retry_policy.retry(|| async {
            let encoded_path = config::encode_job_name(&job_path);
            let url = format!(
                "{}/job/{}/{}/api/json?tree=number,result,building,timestamp,duration,url,fullDisplayName,actions[_class,causes[userName,shortDescription],lastBuiltRevision[SHA1,branch[SHA1,name]],parameters[name,value]]",
                self.server_url, encoded_path, build_number
            );

            tracing::debug!(url = %url, "Fetching Jenkins build details");

            let response = self
                .http_client
                .get(&url)
                .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch build: {e}")))?;

            let response_text = response.text().await
                .map_err(|e| PluginError::ApiError(format!("Failed to read response: {e}")))?;

            tracing::trace!(response = %response_text, "Jenkins raw response");

            let build: types::Build = serde_json::from_str(&response_text)
                .map_err(|e| PluginError::ApiError(format!("Failed to parse build: {e}")))?;

            tracing::debug!(action_count = build.actions.len(), "Parsed Jenkins build");

            Ok(build)
        })
        .await
    }

    pub async fn fetch_build_history(
        &self, job_path: &str, limit: usize,
    ) -> PluginResult<Vec<types::Build>> {
        let encoded_path = config::encode_job_name(job_path);
        let url = format!(
            "{}/job/{}/api/json?tree=builds[number,url,result,building,timestamp,duration,actions[_class,causes[userName,shortDescription],lastBuiltRevision[SHA1,branch[SHA1,name]],parameters[name,value]]]{{0,{limit}}}",
            self.server_url, encoded_path
        );

        let response: types::JobBuildsResponse = self
            .http_client
            .get(&url)
            .header(reqwest::header::AUTHORIZATION, &self.auth_header)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch builds: {e}")))?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse builds: {e}")))?;

        Ok(response.builds)
    }

    pub async fn fetch_job_parameters(
        &self, job_path: &str,
    ) -> PluginResult<types::JobWithParameters> {
        self.retry_policy.retry(|| async {
            let encoded_path = config::encode_job_name(job_path);
            let url = format!(
                "{}/job/{}/api/json?tree=property[parameterDefinitions[name,description,type,defaultParameterValue[value],choices],_class]",
                self.server_url, encoded_path
            );

            tracing::debug!(url = %url, "Fetching Jenkins job parameters");

            let response: types::JobWithParameters = self
                .http_client
                .get(&url)
                .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch job parameters: {e}")))?
                .json()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to parse job parameters: {e}")))?;

            Ok(response)
        })
        .await
    }

    pub async fn trigger_build(
        &self, job_path: &str, form_data: Vec<(String, String)>,
    ) -> PluginResult<()> {
        let job_path = job_path.to_string();
        let form_data_clone = form_data.clone();

        self.retry_policy.retry(|| async {
            let encoded_path = config::encode_job_name(&job_path);
            let has_params = !form_data_clone.is_empty();

            let url = if has_params {
                format!(
                    "{}/job/{}/buildWithParameters",
                    self.server_url, encoded_path
                )
            } else {
                format!("{}/job/{}/build", self.server_url, encoded_path)
            };

            tracing::debug!(url = %url, "Triggering Jenkins build");
            tracing::trace!(form_data = ?form_data_clone, "Jenkins build form data");

            let response = self
                .http_client
                .post(&url)
                .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                .form(&form_data_clone)
                .send()
                .await
                .map_err(|e| {
                    let error_msg = format!("Failed to trigger build: {e}");
                    tracing::error!(error = %e, "Jenkins network error");
                    PluginError::ApiError(error_msg)
                })?;

            let status = response.status();

            if status.is_success() || status == 201 {
                tracing::info!("Jenkins build triggered successfully");
                Ok(())
            } else {
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());

                tracing::error!(status = %status, error = %error_text, "Jenkins trigger failed");

                let params_info = form_data_clone
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            k,
                            if v.len() > 50 {
                                format!("{}...", &v[..50])
                            } else {
                                v.clone()
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                let detailed_error = if error_text.contains("<!DOCTYPE html>")
                    || error_text.contains("<html")
                {
                    format!(
                        "Jenkins returned HTTP {status} error. Check Jenkins console for details. Job: {job_path}, Parameters sent: [{params_info}]"
                    )
                } else {
                    let error_preview = if error_text.len() > 300 {
                        format!("{}...", &error_text[..300])
                    } else {
                        error_text
                    };
                    format!(
                        "HTTP {status}: {error_preview} | Job: {job_path} | Parameters: [{params_info}]"
                    )
                };

                Err(PluginError::ApiError(detailed_error))
            }
        }).await
    }

    pub async fn fetch_pipeline(
        &self, provider_id: i64, job_path: String,
    ) -> PluginResult<Pipeline> {
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

            let build_status = if build.building {
                pipedash_plugin_api::PipelineStatus::Running
            } else {
                mapper::map_jenkins_result(build.result.as_deref())
            };
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
            id: format!("jenkins__{provider_id}__{job_path}"),
            provider_id,
            provider_type: "jenkins".to_string(),
            name: job.name,
            status,
            last_run,
            last_updated: Utc::now(),
            repository: repository_field,
            branch: None,
            workflow_file: None,
            metadata: HashMap::new(),
        })
    }

    pub async fn cancel_build(&self, job_path: &str, build_number: i64) -> PluginResult<()> {
        let job_path = job_path.to_string();

        self.retry_policy
            .retry(|| async {
                let encoded_path = config::encode_job_name(&job_path);
                let url = format!(
                    "{}/job/{}/{}/stop",
                    self.server_url, encoded_path, build_number
                );

                tracing::info!(build_number = build_number, job_path = %job_path, "Cancelling Jenkins build");

                let response = self
                    .http_client
                    .post(&url)
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::ApiError(format!("Failed to cancel build: {e}"))
                    })?;

                let status = response.status();

                if status.is_success() || status == 302 {
                    tracing::info!(build_number = build_number, "Jenkins build cancelled successfully");
                    Ok(())
                } else {
                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    tracing::error!(status = %status, error = %error_text, "Jenkins cancel failed");
                    Err(PluginError::ApiError(format!(
                        "Failed to cancel build: HTTP {status}"
                    )))
                }
            })
            .await
    }

    pub fn discovered_jobs_to_available_pipelines(
        &self, all_jobs: Vec<types::DiscoveredJob>,
    ) -> Vec<AvailablePipeline> {
        all_jobs
            .into_iter()
            .map(|job| {
                let job_type = if job._class.contains("WorkflowMultiBranch") {
                    "Multibranch Pipeline"
                } else if job._class.contains("WorkflowJob") {
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
