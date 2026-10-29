use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::log_dialect::JenkinsLogDialect;
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

    fn job_path(pipeline_id: &str) -> PluginResult<&str> {
        match pipeline_id.split("__").collect::<Vec<_>>().as_slice() {
            [_, _, job_path] => Ok(*job_path),
            _ => Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            ))),
        }
    }

    fn build_number(node_id: &str) -> PluginResult<i64> {
        Some(node_id)
            .filter(|id| id.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("Invalid Jenkins build number: {node_id}"))
            })
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

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let job_path = Self::job_path(run.pipeline_id)?;
        let build = self
            .client()?
            .fetch_build_details(job_path, run.run_number)
            .await?;

        Ok(vec![mapper::build_to_log_node(&build, job_path)])
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let job_path = Self::job_path(run.pipeline_id)?;
        let build_number = Self::build_number(node_id)?;

        self.client()?
            .stream_console_log(job_path, build_number, from, sink)
            .await
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &JenkinsLogDialect
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
        stub_server_with_head,
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

    const BUILD_41: &str = r#"{"number":41,"url":"http://jenkins.example/job/team/job/app/41/","result":"SUCCESS","building":false,"timestamp":1761646800000,"duration":90000,"fullDisplayName":"team » app #41"}"#;
    const BUILD_42: &str = r#"{"number":42,"url":"http://jenkins.example/job/team/job/app/42/","result":null,"building":true,"timestamp":1761647000000,"duration":0,"fullDisplayName":"team » app #42"}"#;
    const CONSOLE_42: &str = "Started by user alice\n[Pipeline] Start of Pipeline\n[Pipeline] stage\n[Pipeline] { (Build)\n[Pipeline] sh\n+ make\nbuilding\n";

    fn folder_job(path: &str) -> Reply {
        let json = |body: &'static str| Reply {
            status: "200 OK",
            headers: "Content-Type: application/json\r\n",
            body,
        };
        let missing = |status: &'static str| Reply {
            status,
            headers: "",
            body: "gone",
        };
        match path.strip_prefix("/job/team/job/app/") {
            Some(rest) if rest.starts_with("41/api/json") => json(BUILD_41),
            Some(rest) if rest.starts_with("42/api/json") => json(BUILD_42),
            Some("42/logText/progressiveText?start=0") => Reply {
                status: "200 OK",
                headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 131\r\nX-More-Data: true\r\n",
                body: CONSOLE_42,
            },
            Some("42/logText/progressiveText?start=131") => Reply {
                status: "200 OK",
                headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 136\r\nX-More-Data: true\r\n",
                body: "done\n",
            },
            Some("42/logText/progressiveText?start=136") => Reply {
                status: "200 OK",
                headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 136\r\nX-More-Data: true\r\n",
                body: "",
            },
            Some("42/logText/progressiveText?start=900") => Reply {
                status: "200 OK",
                headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 131\r\nX-More-Data: true\r\n",
                body: CONSOLE_42,
            },
            Some("42/logText/progressiveText?start=777") => Reply {
                status: "200 OK",
                headers: "Content-Type: text/plain;charset=UTF-8\r\n",
                body: CONSOLE_42,
            },
            Some("8/logText/progressiveText?start=0") => missing("410 Gone"),
            _ => missing("404 Not Found"),
        }
    }

    async fn folder_plugin(requests: Arc<AtomicUsize>) -> JenkinsPlugin {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let expected = format!(
            "Basic {}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "alice:secret")
        );
        let server_url = stub_server_with_head(requests, Duration::ZERO, move |path, head| {
            let authorized = head.lines().any(|line| {
                line.to_ascii_lowercase().starts_with("authorization:") && line.ends_with(&expected)
            });
            if authorized {
                folder_job(path)
            } else {
                Reply {
                    status: "401 Unauthorized",
                    headers: "",
                    body: "denied",
                }
            }
        })
        .await;
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
        plugin
    }

    fn build_of_folder_job(
        run_number: i64, metadata: &HashMap<String, serde_json::Value>,
    ) -> RunRef<'_> {
        RunRef {
            pipeline_id: "jenkins__1__team/app",
            run_number,
            run_id: "jenkins-build-42",
            metadata,
        }
    }

    async fn resume_console(
        plugin: &JenkinsPlugin, run_number: i64, node_id: &str, from: Option<&LogCursor>,
    ) -> PluginResult<(Vec<u8>, LogRead, bool)> {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(
                build_of_folder_job(run_number, &metadata),
                node_id,
                from,
                &mut sink,
            )
            .await;
        let restarted = sink.restarted();
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result.map(|read| (bytes, read, restarted))
    }

    async fn read_console(
        plugin: &JenkinsPlugin, run_number: i64, node_id: &str,
    ) -> PluginResult<Vec<u8>> {
        resume_console(plugin, run_number, node_id, None)
            .await
            .map(|(bytes, _, _)| bytes)
    }

    fn console_cursor(build: i64, offset: u64) -> LogCursor {
        LogCursor::new(&serde_json::json!({ "build": build, "offset": offset })).unwrap()
    }

    #[tokio::test]
    async fn finished_and_running_builds_become_one_job_node_each() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;
        let metadata = HashMap::new();

        let finished = plugin
            .fetch_log_nodes(build_of_folder_job(41, &metadata))
            .await
            .unwrap();
        let running = plugin
            .fetch_log_nodes(build_of_folder_job(42, &metadata))
            .await
            .unwrap();

        assert_eq!(finished.len(), 1);
        let node = &finished[0];
        assert_eq!(node.id, "41");
        assert_eq!(node.parent_id, None);
        assert_eq!(node.kind, LogNodeKind::Job);
        assert_eq!(node.name, "team » app #41");
        assert_eq!(node.status, PipelineStatus::Success);
        assert_eq!(node.log, Some(LogAvailability::Ready));
        assert_eq!(
            node.web_url.as_deref(),
            Some("http://jenkins.example/job/team/job/app/41/")
        );
        assert_eq!(
            node.started_at.unwrap().timestamp_millis(),
            1_761_646_800_000
        );
        assert_eq!(
            node.finished_at.unwrap().timestamp_millis(),
            1_761_646_890_000
        );
        assert_eq!(running.len(), 1);
        let node = &running[0];
        assert_eq!(node.id, "42");
        assert_eq!(node.status, PipelineStatus::Running);
        assert_eq!(node.log, Some(LogAvailability::Partial));
        assert_eq!(node.finished_at, None);
    }

    #[tokio::test]
    async fn build_that_does_not_exist_has_no_log_nodes() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;
        let metadata = HashMap::new();

        let error = plugin
            .fetch_log_nodes(build_of_folder_job(99, &metadata))
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn first_console_read_is_the_whole_log_and_resumes_at_the_server_text_size() {
        let requests = Arc::new(AtomicUsize::new(0));
        let plugin = folder_plugin(requests.clone()).await;

        let (log, read, restarted) = resume_console(&plugin, 42, "42", None).await.unwrap();

        assert_eq!(log, CONSOLE_42.as_bytes());
        assert_eq!(read, LogRead::replace(Some(console_cursor(42, 131))));
        assert!(!restarted);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn resumed_console_read_appends_only_the_new_text() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

        let (log, read, restarted) =
            resume_console(&plugin, 42, "42", Some(&console_cursor(42, 131)))
                .await
                .unwrap();

        assert_eq!(log, b"done\n");
        assert_eq!(read, LogRead::append(console_cursor(42, 136)));
        assert!(!restarted);
    }

    #[tokio::test]
    async fn resumed_console_read_with_nothing_new_writes_nothing() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

        let (log, read, restarted) =
            resume_console(&plugin, 42, "42", Some(&console_cursor(42, 136)))
                .await
                .unwrap();

        assert!(log.is_empty());
        assert_eq!(read, LogRead::append(console_cursor(42, 136)));
        assert!(!restarted);
    }

    #[tokio::test]
    async fn console_that_rolled_over_restarts_with_the_whole_log() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

        let (log, read, restarted) =
            resume_console(&plugin, 42, "42", Some(&console_cursor(42, 900)))
                .await
                .unwrap();

        assert_eq!(log, CONSOLE_42.as_bytes());
        assert_eq!(read, LogRead::replace(Some(console_cursor(42, 131))));
        assert!(restarted);
    }

    #[tokio::test]
    async fn cursor_of_another_build_or_unknown_shape_restarts_from_zero() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;
        let foreign = console_cursor(41, 136);
        let malformed = LogCursor::new(&"junk").unwrap();

        for cursor in [foreign, malformed] {
            let (log, read, restarted) = resume_console(&plugin, 42, "42", Some(&cursor))
                .await
                .unwrap();

            assert_eq!(log, CONSOLE_42.as_bytes(), "{cursor:?}");
            assert_eq!(
                read,
                LogRead::replace(Some(console_cursor(42, 131))),
                "{cursor:?}"
            );
            assert!(restarted, "{cursor:?}");
        }
    }

    #[tokio::test]
    async fn resumed_console_read_without_a_text_size_fails_instead_of_guessing() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

        let error = resume_console(&plugin, 42, "42", Some(&console_cursor(42, 777)))
            .await
            .unwrap_err();

        assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
    }

    #[tokio::test]
    async fn pruned_console_log_is_not_found() {
        let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

        for build in ["7", "8"] {
            let error = read_console(&plugin, 42, build).await.unwrap_err();
            assert!(
                matches!(&error, PluginError::PipelineNotFound(message) if message.contains("no longer available")),
                "{build}: {error:?}"
            );
        }
    }

    #[tokio::test]
    async fn build_numbers_outside_the_digit_alphabet_are_rejected_without_a_request() {
        let requests = Arc::new(AtomicUsize::new(0));
        let plugin = folder_plugin(requests.clone()).await;

        let rejected = [
            "",
            "../../api/json",
            "42/consoleText",
            "42?tree=x",
            "4 2",
            "-1",
            "+1",
            "٤٢",
            "99999999999999999999",
        ];
        for node_id in rejected {
            let error = read_console(&plugin, 42, node_id).await.unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{node_id}: {error:?}"
            );
        }
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }
}
