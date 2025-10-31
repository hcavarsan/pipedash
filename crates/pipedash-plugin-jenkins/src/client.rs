//! Jenkins API client and methods

use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PluginError,
    PluginResult,
};
use reqwest::Client;

use crate::{
    config,
    mapper,
    types,
};

/// Jenkins API client with retry logic
pub(crate) struct JenkinsClient {
    pub(crate) client: Client,
    server_url: String,
}

impl JenkinsClient {
    pub fn new(client: Client, server_url: String) -> Self {
        Self { client, server_url }
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Retries a request operation with exponential backoff
    pub(crate) async fn retry_request<F, Fut, T>(&self, operation: F) -> PluginResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = PluginResult<T>>,
    {
        let max_retries = 2;
        let mut delay = Duration::from_millis(50);
        let mut last_error = None;

        for attempt in 0..max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < max_retries - 1 => match &e {
                    PluginError::NetworkError(_) | PluginError::ApiError(_) => {
                        eprintln!(
                            "[JENKINS] Retry attempt {} after error: {:?}",
                            attempt + 1,
                            e
                        );
                        last_error = Some(e);
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                        continue;
                    }
                    _ => return Err(e),
                },
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| PluginError::NetworkError("Max retries exceeded".to_string())))
    }

    /// Discovers all jobs recursively
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

    /// Fetches jobs in a specific folder
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
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch jobs: {e}")))?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse jobs: {e}")))?;

        Ok(response.jobs)
    }

    /// Fetches job details
    pub async fn fetch_job_details(&self, job_path: &str) -> PluginResult<types::Job> {
        let job_path = job_path.to_string();
        self.retry_request(|| async {
            let encoded_path = config::encode_job_name(&job_path);
            let url = format!(
                "{}/job/{}/api/json?tree=name,lastBuild[number]",
                self.server_url, encoded_path
            );

            let job: types::Job = self
                .client
                .get(&url)
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch job {job_path}: {e}")))?
                .json()
                .await
                .map_err(|e| {
                    PluginError::ApiError(format!("Failed to parse job {job_path}: {e}"))
                })?;

            Ok(job)
        })
        .await
    }

    /// Fetches build details (always fresh, no caching)
    /// Includes all build information: status, timestamps, actions with
    /// parameters, git info
    pub async fn fetch_build_details(
        &self, job_path: &str, build_number: i64,
    ) -> PluginResult<types::Build> {
        let job_path = job_path.to_string();
        self.retry_request(|| async {
            let encoded_path = config::encode_job_name(&job_path);
            let url = format!(
                "{}/job/{}/{}/api/json?tree=number,result,building,timestamp,duration,url,fullDisplayName,actions[_class,causes[userName,shortDescription],lastBuiltRevision[SHA1,branch[SHA1,name]],parameters[name,value]]",
                self.server_url, encoded_path, build_number
            );

            eprintln!("[JENKINS CLIENT] Fetching build details from: {url}");

            let response = self
                .client
                .get(&url)
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch build: {e}")))?;

            let response_text = response.text().await
                .map_err(|e| PluginError::ApiError(format!("Failed to read response: {e}")))?;

            eprintln!("[JENKINS CLIENT] Raw response: {response_text}");

            let build: types::Build = serde_json::from_str(&response_text)
                .map_err(|e| PluginError::ApiError(format!("Failed to parse build: {e}")))?;

            eprintln!("[JENKINS CLIENT] Parsed build with {} actions", build.actions.len());

            Ok(build)
        })
        .await
    }

    /// Fetches build history for a job
    pub async fn fetch_build_history(
        &self, job_path: &str, limit: usize,
    ) -> PluginResult<Vec<types::Build>> {
        let encoded_path = config::encode_job_name(job_path);
        let url = format!(
            "{}/job/{}/api/json?tree=builds[number,url,result,building,timestamp,duration,actions[causes[userName],lastBuiltRevision[SHA1,branch[SHA1,name]]]]{{0,{limit}}}",
            self.server_url, encoded_path
        );

        let response: types::JobBuildsResponse = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch builds: {e}")))?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse builds: {e}")))?;

        Ok(response.builds)
    }

    /// Fetches workflow parameters for a job
    pub async fn fetch_job_parameters(
        &self, job_path: &str,
    ) -> PluginResult<types::JobWithParameters> {
        self.retry_request(|| async {
            let encoded_path = config::encode_job_name(job_path);
            let url = format!(
                "{}/job/{}/api/json?tree=property[parameterDefinitions[name,description,type,defaultParameterValue[value],choices],_class]",
                self.server_url, encoded_path
            );

            eprintln!("[JENKINS] Fetching params from: {url}");

            let response: types::JobWithParameters = self
                .client
                .get(&url)
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

    /// Triggers a build
    pub async fn trigger_build(
        &self, job_path: &str, form_data: Vec<(String, String)>,
    ) -> PluginResult<()> {
        let encoded_path = config::encode_job_name(job_path);
        let has_params = !form_data.is_empty();

        let url = if has_params {
            format!(
                "{}/job/{}/buildWithParameters",
                self.server_url, encoded_path
            )
        } else {
            format!("{}/job/{}/build", self.server_url, encoded_path)
        };

        eprintln!("Triggering Jenkins build - URL: {url}");
        eprintln!("Form data: {form_data:?}");

        let response = self
            .client
            .post(&url)
            .form(&form_data)
            .send()
            .await
            .map_err(|e| {
                let error_msg = format!("Failed to trigger build: {e}");
                eprintln!("Network error: {error_msg}");
                PluginError::ApiError(error_msg)
            })?;

        let status = response.status();

        if status.is_success() || status == 201 {
            eprintln!("Jenkins build triggered successfully");
            Ok(())
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            eprintln!("Jenkins trigger failed - Status: {status}, Error: {error_text}");

            let params_info = form_data
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
    }

    /// Fetches a single pipeline with its status
    pub async fn fetch_pipeline(
        &self, provider_id: i64, job_path: String,
    ) -> PluginResult<Pipeline> {
        let pipeline_start = std::time::Instant::now();
        eprintln!("[JENKINS] Fetching pipeline: {job_path}");

        let job_start = std::time::Instant::now();
        let job = self.fetch_job_details(&job_path).await?;
        eprintln!("[JENKINS] Fetched job details in {:?}", job_start.elapsed());

        let (status, last_run) = if let Some(ref last_build) = job.last_build {
            let build_start = std::time::Instant::now();
            let build = self
                .fetch_build_details(&job_path, last_build.number)
                .await?;
            eprintln!(
                "[JENKINS] Fetched build details in {:?}",
                build_start.elapsed()
            );

            let build_status = if build.building {
                pipedash_plugin_api::PipelineStatus::Running
            } else {
                mapper::map_jenkins_result(build.result.as_deref())
            };
            let build_time = chrono::DateTime::from_timestamp_millis(build.timestamp)
                .map(|dt| dt.with_timezone(&Utc));
            (build_status, build_time)
        } else {
            eprintln!("[JENKINS] No last build found");
            (pipedash_plugin_api::PipelineStatus::Pending, None)
        };

        let (org, repo) = config::split_job_path(&job_path);
        let repository_field = if job_path.contains('/') {
            job_path.clone()
        } else {
            format!("{org}/{repo}")
        };

        eprintln!(
            "[JENKINS] Total pipeline fetch time: {:?}",
            pipeline_start.elapsed()
        );

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
        })
    }

    /// Cancels a running build
    pub async fn cancel_build(&self, job_path: &str, build_number: i64) -> PluginResult<()> {
        let encoded_path = config::encode_job_name(job_path);
        let url = format!(
            "{}/job/{}/{}/stop",
            self.server_url, encoded_path, build_number
        );

        eprintln!("[JENKINS] Cancelling build #{build_number} for job {job_path}");

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to cancel build: {e}")))?;

        let status = response.status();

        if status.is_success() || status == 302 {
            eprintln!("[JENKINS] Build #{build_number} cancelled successfully");
            Ok(())
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            eprintln!("[JENKINS] Cancel failed - Status: {status}, Error: {error_text}");
            Err(PluginError::ApiError(format!(
                "Failed to cancel build: HTTP {status}"
            )))
        }
    }

    /// Converts discovered jobs to available pipelines
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
