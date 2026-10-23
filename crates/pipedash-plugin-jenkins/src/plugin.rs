use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    mapper,
    metadata,
};

pub struct JenkinsPlugin {
    metadata: PluginMetadata,
    client: Option<client::JenkinsClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
}

impl Default for JenkinsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl JenkinsPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
        }
    }

    fn client(&self) -> PluginResult<&client::JenkinsClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }
}

#[async_trait]
impl Plugin for JenkinsPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Jenkins API token".to_string()))?;

        let username = config
            .get("username")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Jenkins username".to_string()))?;

        let auth_value = format!("{username}:{token}");
        let auth_header = format!(
            "Basic {}",
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                auth_value.as_bytes()
            )
        );

        let server_url = config
            .get("server_url")
            .ok_or_else(|| PluginError::InvalidConfig("Missing server_url".to_string()))?
            .trim_end_matches('/')
            .to_string();

        let client = http_client.unwrap_or_else(|| {
            std::sync::Arc::new(
                reqwest::Client::builder()
                    .use_rustls_tls()
                    .pool_max_idle_per_host(10)
                    .timeout(Duration::from_secs(30))
                    .connect_timeout(Duration::from_secs(10))
                    .tcp_keepalive(Duration::from_secs(60))
                    .build()
                    .expect("Failed to build HTTP client"),
            )
        });

        self.client = Some(client::JenkinsClient::new(client, server_url, auth_header));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        client.discover_all_jobs().await?;
        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let client = self.client()?;
        let all_jobs = client.discover_all_jobs().await?;
        let all_pipelines = client.discovered_jobs_to_available_pipelines(all_jobs);

        let total_count = all_pipelines.len();
        let start = ((params.page - 1) * params.page_size).min(total_count);
        let end = (start + params.page_size).min(total_count);
        let items = all_pipelines[start..end].to_vec();

        Ok(PaginatedResponse::new(
            items,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let job_paths = config::parse_selected_items(&self.config)?;

        if job_paths.is_empty() {
            return Err(PluginError::InvalidConfig("No jobs configured".to_string()));
        }

        let client = self.client()?;
        let futures = job_paths
            .iter()
            .map(|job_path| client.fetch_pipeline(provider_id, job_path.clone()));

        let results = join_all(futures).await;

        let mut all_pipelines = Vec::new();
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(pipeline) => all_pipelines.push(pipeline),
                Err(e) => errors.push(e),
            }
        }

        if !errors.is_empty() && all_pipelines.is_empty() {
            return Err(errors.into_iter().next().unwrap());
        }

        Ok(all_pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        let builds = client.fetch_build_history(job_path, limit).await?;

        let pipeline_runs = builds
            .into_iter()
            .map(|build| {
                let encoded_path = config::encode_job_name(job_path);
                mapper::build_to_pipeline_run(
                    build,
                    pipeline_id,
                    client.server_url(),
                    &encoded_path,
                )
            })
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        let build = client.fetch_build_details(job_path, run_number).await?;

        let encoded_path = config::encode_job_name(job_path);
        Ok(mapper::build_to_pipeline_run(
            build,
            pipeline_id,
            client.server_url(),
            &encoded_path,
        ))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let parts: Vec<&str> = params.workflow_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {}",
                params.workflow_id
            )));
        }

        let job_path = parts[2];

        let mut form_data = Vec::new();
        if let Some(inputs) = params.inputs {
            if let Some(obj) = inputs.as_object() {
                for (k, v) in obj.iter() {
                    if v.is_null() {
                        continue;
                    }

                    if v.is_array() {
                        if let Some(arr) = v.as_array() {
                            for item in arr {
                                if item.is_null() {
                                    continue;
                                }
                                let value_str = if item.is_boolean() {
                                    item.as_bool().unwrap().to_string()
                                } else if item.is_number() {
                                    item.to_string()
                                } else {
                                    item.as_str()
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| item.to_string())
                                };
                                form_data.push((k.clone(), value_str));
                            }
                        }
                    } else if v.is_boolean() {
                        form_data.push((k.clone(), v.as_bool().unwrap().to_string()));
                    } else if v.is_number() {
                        form_data.push((k.clone(), v.to_string()));
                    } else if let Some(s) = v.as_str() {
                        form_data.push((k.clone(), s.to_string()));
                    }
                }
            }
        }

        if form_data.is_empty()
            && !self
                .fetch_workflow_parameters(&params.workflow_id)
                .await?
                .is_empty()
        {
            form_data.push(("json".to_string(), serde_json::json!({}).to_string()));
        }

        let client = self.client()?;
        client.trigger_build(job_path, form_data).await?;

        Ok(serde_json::json!({
            "message": format!("Triggered build for job {job_path}"),
            "job_path": job_path
        })
        .to_string())
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let start = std::time::Instant::now();
        tracing::debug!(workflow_id = %workflow_id, "Fetching Jenkins workflow parameters");

        let parts: Vec<&str> = workflow_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {workflow_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        let response = client.fetch_job_parameters(job_path).await?;

        let param_definitions: Vec<_> = response
            .property
            .into_iter()
            .filter(|prop| {
                prop._class
                    .as_ref()
                    .map(|c| {
                        c.contains("ParametersDefinitionProperty")
                            || c.contains("ParametersProperty")
                    })
                    .unwrap_or(true)
            })
            .flat_map(|prop| prop.parameter_definitions)
            .collect();

        let parameters = mapper::parameter_definitions_to_workflow_parameters(param_definitions);

        tracing::debug!(
            count = parameters.len(),
            elapsed = ?start.elapsed(),
            "Processed Jenkins workflow parameters"
        );
        Ok(parameters)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        Ok(vec![Organization {
            id: "default".to_string(),
            name: "Jenkins Server".to_string(),
            description: Some("All accessible Jenkins jobs".to_string()),
        }])
    }

    async fn fetch_available_pipelines_filtered(
        &self, _org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let client = self.client()?;
        let all_jobs = client.discover_all_jobs().await?;
        let mut all_pipelines = client.discovered_jobs_to_available_pipelines(all_jobs);

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            all_pipelines.retain(|p| {
                p.name.to_lowercase().contains(&search_lower)
                    || p.id.to_lowercase().contains(&search_lower)
                    || p.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&search_lower))
            });
        }

        let total_count = all_pipelines.len();
        let start = ((params.page - 1) * params.page_size).min(total_count);
        let end = (start + params.page_size).min(total_count);
        let items = all_pipelines[start..end].to_vec();

        Ok(PaginatedResponse::new(
            items,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        client.cancel_build(job_path, run_number).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::sync::Mutex;

    use super::*;

    async fn jenkins_stub(requests: Arc<Mutex<Vec<String>>>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..n]);
                }
                let line = String::from_utf8_lossy(&request)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                requests.lock().await.push(line.clone());

                let (status, body) = if line.starts_with("GET /job/plain/api/json") {
                    (
                        "200 OK",
                        r#"{"property":[{"_class":"jenkins.model.BuildDiscarderProperty"}]}"#,
                    )
                } else if line.starts_with("GET /job/release/api/json") {
                    (
                        "200 OK",
                        r#"{"property":[{"_class":"hudson.model.ParametersDefinitionProperty","parameterDefinitions":[{"_class":"hudson.model.StringParameterDefinition","name":"BRANCH","defaultParameterValue":{"value":"main"}}]}]}"#,
                    )
                } else if line.starts_with("POST /job/plain/build ")
                    || line.starts_with("POST /job/release/buildWithParameters ")
                {
                    ("201 Created", "")
                } else {
                    ("400 Bad Request", "job is not parameterized")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        format!("http://{addr}")
    }

    async fn trigger_without_inputs(job: &str) -> (PluginResult<String>, Vec<String>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server_url = jenkins_stub(requests.clone()).await;
        let mut plugin = JenkinsPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("server_url".to_string(), server_url),
                    ("username".to_string(), "alice".to_string()),
                    ("token".to_string(), "secret".to_string()),
                ]),
                None,
            )
            .unwrap();

        let result = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: format!("jenkins__1__{job}"),
                inputs: None,
            })
            .await;
        let requests = requests.lock().await.clone();

        (result, requests)
    }

    fn posts(requests: &[String]) -> Vec<&str> {
        requests
            .iter()
            .filter(|line| line.starts_with("POST "))
            .map(String::as_str)
            .collect()
    }

    #[tokio::test]
    async fn trigger_posts_to_build_for_a_job_without_parameters() {
        let (result, requests) = trigger_without_inputs("plain").await;

        assert!(result.is_ok(), "{result:?}, requests: {requests:?}");
        assert_eq!(posts(&requests), ["POST /job/plain/build HTTP/1.1"]);
    }

    #[tokio::test]
    async fn trigger_posts_to_build_with_parameters_for_a_parameterized_job() {
        let (result, requests) = trigger_without_inputs("release").await;

        assert!(result.is_ok(), "{result:?}, requests: {requests:?}");
        assert_eq!(
            posts(&requests),
            ["POST /job/release/buildWithParameters HTTP/1.1"]
        );
    }
}
