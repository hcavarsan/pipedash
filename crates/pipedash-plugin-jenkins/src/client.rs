use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    AvailablePipeline,
    Pipeline,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::{
    Client,
    Url,
};
use serde::de::DeserializeOwned;

use crate::{
    config,
    mapper,
    types,
};

fn parse_url(url: &str) -> PluginResult<Url> {
    Url::parse(url).map_err(|e| PluginError::InvalidConfig(format!("Invalid Jenkins URL: {e}")))
}

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

    async fn get_json<T: DeserializeOwned>(
        &self, url: &str, timeout: Option<Duration>,
    ) -> PluginResult<T> {
        let url = parse_url(url)?;

        self.retry_policy
            .retry(|| async {
                let mut request = self
                    .http_client
                    .get(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header);
                if let Some(timeout) = timeout {
                    request = request.timeout(timeout);
                }
                let response = request.send().await.map_err(|e| map_send_error(e, &url))?;

                check_response(response)
                    .await?
                    .json()
                    .await
                    .map_err(|e| map_send_error(e, &url))
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
        let url = parse_url(&format!(
            "{}/job/{}/{endpoint}",
            self.server_url, encoded_path
        ))?;
        let parameter_names = form_data
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        tracing::debug!(job_path = %job_path, parameters = %parameter_names, "Triggering Jenkins build");

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .form(&form_data)
                    .send()
            })
            .await?;
        check_response(response).await.inspect_err(|e| {
            tracing::error!(job_path = %job_path, parameters = %parameter_names, error = %e, "Jenkins trigger failed");
        })?;

        tracing::info!("Jenkins build triggered successfully");
        Ok(())
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
        let encoded_path = config::encode_job_name(job_path);
        let url = parse_url(&format!(
            "{}/job/{}/{}/stop",
            self.server_url, encoded_path, build_number
        ))?;

        tracing::info!(build_number = build_number, job_path = %job_path, "Cancelling Jenkins build");

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                    .send()
            })
            .await?;
        check_response(response).await?;

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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use std::sync::Arc;

    use super::*;
    use crate::test_support::{
        stub_server,
        Reply,
    };

    fn unauthorized(_: &str) -> Reply {
        Reply {
            status: "401 Unauthorized",
            headers: "Content-Type: text/html\r\n",
            body: "<html><body><h2>HTTP ERROR 401 Unauthorized</h2></body></html>",
        }
    }

    fn not_found(_: &str) -> Reply {
        Reply {
            status: "404 Not Found",
            headers: "",
            body: "<html><body><h2>HTTP ERROR 404 Not Found</h2></body></html>",
        }
    }

    fn rate_limited(_: &str) -> Reply {
        Reply {
            status: "429 Too Many Requests",
            headers: "Retry-After: 120\r\n",
            body: "slow down",
        }
    }

    fn unavailable(_: &str) -> Reply {
        Reply {
            status: "503 Service Unavailable",
            headers: "",
            body: "maintenance",
        }
    }

    fn created(_: &str) -> Reply {
        Reply {
            status: "201 Created",
            headers: "",
            body: "",
        }
    }

    async fn client_for(
        reply: fn(&str) -> Reply, delay: Duration,
    ) -> (JenkinsClient, Arc<AtomicUsize>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(AtomicUsize::new(0));
        let server_url = stub_server(requests.clone(), delay, reply).await;
        let http_client = Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let client = JenkinsClient::new(
            Arc::new(http_client),
            server_url,
            "Basic YWxpY2U6d3Jvbmc=".to_string(),
        );
        (client, requests)
    }

    #[tokio::test]
    async fn rejected_credentials_fail_as_auth_error_without_retry() {
        let (client, requests) = client_for(unauthorized, Duration::ZERO).await;

        let results = [
            client.discover_all_jobs().await.err(),
            client.fetch_job_details("build").await.err(),
            client.fetch_build_details("build", 1).await.err(),
            client.fetch_build_history("build", 10).await.err(),
            client.fetch_job_parameters("build").await.err(),
        ];

        for error in results {
            assert!(
                matches!(error, Some(PluginError::AuthenticationFailed(_))),
                "{error:?}"
            );
        }
        assert_eq!(requests.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn missing_job_is_pipeline_not_found() {
        let (client, requests) = client_for(not_found, Duration::ZERO).await;

        let error = client.fetch_job_details("gone").await.unwrap_err();

        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{error:?}"
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rate_limit_carries_retry_after_and_is_not_retried() {
        let (client, requests) = client_for(rate_limited, Duration::ZERO).await;

        let error = client.fetch_job_details("build").await.unwrap_err();

        assert!(
            matches!(error, PluginError::RateLimited { .. }),
            "{error:?}"
        );
        assert_eq!(error.retry_after(), Some(Duration::from_secs(120)));
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn server_error_is_service_unavailable_and_retried() {
        let (client, requests) = client_for(unavailable, Duration::ZERO).await;

        let error = client.fetch_job_details("build").await.unwrap_err();

        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error:?}"
        );
        assert_eq!(requests.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn timed_out_mutations_are_not_sent_again() {
        let (client, requests) = client_for(created, Duration::from_secs(2)).await;

        let trigger = client
            .trigger_build("build", vec![("name".to_string(), "value".to_string())])
            .await
            .unwrap_err();
        let cancel = client.cancel_build("build", 7).await.unwrap_err();

        assert!(
            matches!(trigger, PluginError::NetworkError(_)),
            "{trigger:?}"
        );
        assert!(matches!(cancel, PluginError::NetworkError(_)), "{cancel:?}");
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failed_trigger_error_has_no_parameter_values() {
        let (client, requests) = client_for(unavailable, Duration::ZERO).await;
        let secret = "s3cr3t-".repeat(20);

        let error = client
            .trigger_build(
                "build",
                vec![
                    ("DEPLOY_TOKEN".to_string(), secret.clone()),
                    ("short".to_string(), "hunter2".to_string()),
                ],
            )
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error:?}"
        );
        let text = error.to_string();
        assert!(
            !text.contains("s3cr3t") && !text.contains("hunter2"),
            "{text}"
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }
}
