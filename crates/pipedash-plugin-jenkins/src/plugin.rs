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
    status_source: Option<StatusSource>,
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
            status_source: None,
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

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let status_source = custom_status_source(&config)?;

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
        self.status_source = status_source;

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
                Err(e) if e.is_transient() => return Err(e),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to fetch Jenkins pipeline");
                    errors.push(e);
                }
            }
        }

        if all_pipelines.is_empty() {
            if let Some(e) = errors.into_iter().next() {
                return Err(e);
            }
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

    fn mixed(path: &str) -> Reply {
        if path.contains("/job/limited/") {
            Reply {
                status: "429 Too Many Requests",
                headers: "Retry-After: 120\r\n",
                body: "slow down",
            }
        } else if path.contains("/job/missing/") {
            Reply {
                status: "404 Not Found",
                headers: "",
                body: "not found",
            }
        } else {
            Reply {
                status: "200 OK",
                headers: "Content-Type: application/json\r\n",
                body: r#"{"name":"ok"}"#,
            }
        }
    }

    async fn initialized(selected: &str, extra: &[(&str, &str)]) -> PluginResult<JenkinsPlugin> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let server_url = stub_server(Arc::new(AtomicUsize::new(0)), Duration::ZERO, mixed).await;
        let mut config = HashMap::from([
            ("server_url".to_string(), server_url),
            ("username".to_string(), "alice".to_string()),
            ("token".to_string(), "token".to_string()),
            ("selected_items".to_string(), selected.to_string()),
        ]);
        config.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        let mut plugin = JenkinsPlugin::new();
        plugin.initialize(1, config, None)?;
        Ok(plugin)
    }

    #[tokio::test]
    async fn transient_failure_on_any_job_fails_the_fetch() {
        let plugin = initialized("ok,limited", &[]).await.unwrap();

        let error = plugin.fetch_pipelines().await.unwrap_err();

        assert!(
            matches!(error, PluginError::RateLimited { .. }),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn permanent_failure_on_one_job_keeps_the_rest() {
        let plugin = initialized("ok,missing", &[]).await.unwrap();

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        assert_eq!(pipelines.len(), 1);
    }

    #[tokio::test]
    async fn status_source_follows_the_optional_status_page_url() {
        let none = initialized("ok", &[]).await.unwrap();
        assert!(none.status_source().is_none());

        let custom = initialized("ok", &[("status_page_url", "https://status.example.com")])
            .await
            .unwrap();
        let source = custom.status_source().unwrap();
        assert_eq!(source.name, "status.example.com");
        assert_eq!(
            source.feed_url.as_str(),
            "https://status.example.com/api/v2/summary.json"
        );

        let rejected = initialized("ok", &[("status_page_url", "javascript:alert(1)")]).await;
        assert!(matches!(rejected, Err(PluginError::InvalidConfig(_))));
    }

    fn jenkins_core(path: &str) -> Reply {
        let json = "Content-Type: application/json\r\n";
        if path.starts_with("/job/plain/api/json") {
            Reply {
                status: "200 OK",
                headers: json,
                body: r#"{"property":[{"_class":"jenkins.model.BuildDiscarderProperty"}]}"#,
            }
        } else if path.starts_with("/job/release/api/json") {
            Reply {
                status: "200 OK",
                headers: json,
                body: r#"{"property":[{"_class":"hudson.model.ParametersDefinitionProperty","parameterDefinitions":[{"_class":"hudson.model.StringParameterDefinition","name":"BRANCH","defaultParameterValue":{"value":"main"}}]}]}"#,
            }
        } else if path == "/job/plain/build" || path == "/job/release/buildWithParameters" {
            Reply {
                status: "201 Created",
                headers: "",
                body: "",
            }
        } else {
            Reply {
                status: "400 Bad Request",
                headers: "",
                body: "job is not parameterized",
            }
        }
    }

    async fn trigger_without_inputs(job: &str) -> (PluginResult<String>, usize) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(AtomicUsize::new(0));
        let server_url = stub_server(requests.clone(), Duration::ZERO, jenkins_core).await;
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

        (result, requests.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn trigger_posts_to_build_for_a_job_without_parameters() {
        let (result, requests) = trigger_without_inputs("plain").await;

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(requests, 2);
    }

    #[tokio::test]
    async fn trigger_posts_to_build_with_parameters_for_a_parameterized_job() {
        let (result, requests) = trigger_without_inputs("release").await;

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(requests, 2);
    }
}
