use std::collections::{
    BTreeSet,
    HashMap,
};
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::log_dialect::BuildkiteLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    job: String,
    offset: u64,
}

pub struct BuildkitePlugin {
    metadata: PluginMetadata,
    client: Option<client::BuildkiteClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    status_source: Option<StatusSource>,
}

impl Default for BuildkitePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildkitePlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            status_source: None,
        }
    }

    fn client(&self) -> PluginResult<&client::BuildkiteClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn org_and_slug(pipeline_id: &str) -> PluginResult<(&str, &str)> {
        match pipeline_id.split("__").collect::<Vec<_>>().as_slice() {
            [_, _, org, slug] => Ok((*org, *slug)),
            _ => Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            ))),
        }
    }
}

fn buildkite_status_source() -> PluginResult<StatusSource> {
    let url = |value: &str| {
        reqwest::Url::parse(value)
            .map_err(|e| PluginError::Internal(format!("Invalid Buildkite status URL: {e}")))
    };

    Ok(StatusSource {
        name: "Buildkite".to_string(),
        page_url: url("https://www.buildkitestatus.com/")?,
        feed_url: url("https://www.buildkitestatus.com/api/v2/summary.json")?,
        format: FeedFormat::Statuspage,
        components: ComponentFilter::Ids(
            [
                "r21z46hqn47c",
                "1mpkbbd1kh0h",
                "3n7fzwsysx86",
                "56ph4s45xl9s",
            ]
            .map(String::from)
            .to_vec(),
        ),
    })
}

#[async_trait]
impl Plugin for BuildkitePlugin {
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
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Buildkite API token".to_string()))?
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

        self.client = Some(client::BuildkiteClient::new(client, token));
        self.provider_id = Some(provider_id);
        self.config = config;
        self.status_source = Some(buildkite_status_source()?);

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        client.fetch_organizations().await?;

        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        client::fetch_all_available_pipelines(client, params).await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let client = self.client()?;
        let orgs = client.fetch_organizations().await?;

        Ok(orgs
            .into_iter()
            .map(|org| pipedash_plugin_api::Organization {
                id: org.slug.clone(),
                name: org.name,
                description: org.description,
            })
            .collect())
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        client::fetch_available_pipelines_filtered(client, org, search, params).await
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let pipelines = config::parse_selected_items(&self.config)?;

        if pipelines.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No pipelines configured".to_string(),
            ));
        }

        let client = self.client()?;
        let futures = pipelines
            .into_iter()
            .map(|(org, slug)| client.fetch_pipeline(provider_id, org, slug));

        let results = join_all(futures).await;

        let mut all_pipelines = Vec::new();
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(pipeline) => all_pipelines.push(pipeline),
                Err(e) if e.is_transient() => return Err(e),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to fetch Buildkite pipeline");
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
        if parts.len() != 4 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let org = parts[2];
        let slug = parts[3];

        let client = self.client()?;
        let builds = client.fetch_builds(org, slug, limit).await?;

        let pipeline_runs = builds
            .into_iter()
            .map(|build| client::build_to_pipeline_run(build, pipeline_id))
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (org, slug) = Self::org_and_slug(pipeline_id)?;
        let build = self.client()?.fetch_build(org, slug, run_number).await?;

        Ok(client::build_to_pipeline_run(build, pipeline_id))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (org, slug) = Self::org_and_slug(run.pipeline_id)?;
        let jobs = self
            .client()?
            .fetch_build_jobs(org, slug, run.run_number)
            .await?;

        Ok(mapper::jobs_to_log_nodes(&jobs))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (org, slug) = Self::org_and_slug(run.pipeline_id)?;
        if node_id.is_empty() || !node_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid Buildkite job ID: {node_id}"
            )));
        }

        let position = from
            .and_then(|cursor| cursor.parse::<LogPosition>())
            .filter(|position| position.job == node_id);
        if from.is_some() && position.is_none() {
            sink.restart()?;
        }
        let (mode, offset) = self
            .client()?
            .stream_job_log(
                org,
                slug,
                run.run_number,
                node_id,
                position.map(|p| p.offset),
                sink,
            )
            .await?;
        let job = node_id.to_string();
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition { job, offset })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &BuildkiteLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![WorkflowParameter {
            name: "branch".to_string(),
            label: Some("Branch".to_string()),
            description: Some("Branch to build".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let parts: Vec<&str> = params.workflow_id.split("__").collect();
        if parts.len() != 4 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {}",
                params.workflow_id
            )));
        }

        let org = parts[2];
        let slug = parts[3];

        let branch = params
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get("branch"))
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let client = self.client()?;
        let build = client
            .trigger_build(org, slug, branch.clone(), params.inputs)
            .await?;

        Ok(serde_json::json!({
            "message": format!("Triggered build #{} on branch {}", build.number, branch),
            "build_number": build.number,
            "build_url": build.web_url
        })
        .to_string())
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        let orgs: BTreeSet<String> = config::parse_selected_items(&self.config)?
            .into_iter()
            .map(|(org, _)| org)
            .collect();

        let client = self.client()?;
        let mut agents = Vec::new();
        for org in orgs {
            agents.extend(client.fetch_agents(&org).await?);
        }

        Ok(agents.into_iter().map(mapper::map_agent).collect())
    }

    async fn fetch_artifacts(&self, run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        let parts: Vec<&str> = run_id.split("__").collect();
        let [_, _, org, _, build_id] = parts.as_slice() else {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid run ID: {run_id}"
            )));
        };

        let client = self.client()?;
        let artifacts = client.fetch_artifacts(org, build_id).await?;

        Ok(artifacts
            .into_iter()
            .map(|artifact| client::artifact_to_build_artifact(artifact, run_id))
            .collect())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 4 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let org = parts[2];
        let slug = parts[3];

        let client = self.client()?;
        client.cancel_build(org, slug, run_number).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::test_support::{
        stub_server,
        Reply,
    };

    fn mixed(path: &str) -> Reply {
        if path.contains("/limited") {
            Reply {
                status: "429 Too Many Requests",
                headers: "Retry-After: 120\r\n",
                body: "slow down",
            }
        } else if path.contains("/missing") {
            Reply {
                status: "404 Not Found",
                headers: "",
                body: "{}",
            }
        } else if path.contains("/builds") {
            Reply {
                status: "200 OK",
                headers: "Content-Type: application/json\r\n",
                body: "[]",
            }
        } else {
            Reply {
                status: "200 OK",
                headers: "Content-Type: application/json\r\n",
                body: r#"{"id":"1","slug":"ok","name":"ok","url":"u","repository":"git@github.com:acme/ok.git"}"#,
            }
        }
    }

    const BUILD_PATH: &str = "/organizations/acme/pipelines/app/builds/150";
    const JOB_ID: &str = "0190d2a4-7b1e-4c55-9f0e-6f2a1c3b4d5e";
    const RAW_LOG: &str = "\x1b_bk;t=1745322210692\x07~~~ Running script\r\n\
                           \x1b_bk;t=1745322210692\x07\x1b[90m$\x1b[0m ./script.sh\r\n";

    fn build_150(path: &str) -> Reply {
        let json = |body| Reply {
            status: "200 OK",
            headers: "Content-Type: application/json\r\n",
            body,
        };
        match path.strip_prefix(BUILD_PATH) {
            Some("?exclude_jobs=true") => json(
                r#"{"id":"b-150","number":150,"state":"failed","web_url":"https://buildkite.com/acme/app/builds/150",
                    "created_at":"2026-10-27T10:00:00.000Z","started_at":"2026-10-27T10:00:05.000Z",
                    "finished_at":"2026-10-27T10:02:05.000Z","branch":"main","commit":"abc"}"#,
            ),
            Some("") => json(
                r#"{"number":150,"jobs":[{"id":"0190d2a4-7b1e-4c55-9f0e-6f2a1c3b4d5e","type":"script",
                    "name":"Tests","state":"failed","started_at":"2026-10-27T10:00:05.000Z",
                    "finished_at":"2026-10-27T10:02:05.000Z"}]}"#,
            ),
            Some("/jobs/0190d2a4-7b1e-4c55-9f0e-6f2a1c3b4d5e/log") => Reply {
                status: "200 OK",
                headers: "Content-Type: text/plain\r\n",
                body: RAW_LOG,
            },
            _ => Reply {
                status: "404 Not Found",
                headers: "Content-Type: application/json\r\n",
                body: r#"{"message":"Not Found"}"#,
            },
        }
    }

    async fn plugin_answering(
        reply: fn(&str) -> Reply, selected: &str,
    ) -> (BuildkitePlugin, Arc<AtomicUsize>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(AtomicUsize::new(0));
        let base_url = stub_server(requests.clone(), Duration::ZERO, reply).await;
        let mut plugin = BuildkitePlugin::new();
        let config = HashMap::from([
            ("token".to_string(), "token".to_string()),
            ("selected_items".to_string(), selected.to_string()),
        ]);
        plugin.initialize(1, config, None).unwrap();
        plugin.client = Some(
            client::BuildkiteClient::new(Arc::new(reqwest::Client::new()), "token".to_string())
                .with_base_url(base_url),
        );
        (plugin, requests)
    }

    async fn plugin_selecting(selected: &str) -> BuildkitePlugin {
        plugin_answering(mixed, selected).await.0
    }

    fn run_150(metadata: &HashMap<String, serde_json::Value>) -> RunRef<'_> {
        RunRef {
            pipeline_id: "buildkite__1__acme__app",
            run_number: 150,
            run_id: "buildkite__1__acme__app__b-150",
            metadata,
        }
    }

    async fn read(plugin: &BuildkitePlugin, node_id: &str) -> PluginResult<Vec<u8>> {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_150(&metadata), node_id, None, &mut sink)
            .await;
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result.map(|_| bytes)
    }

    #[tokio::test]
    async fn run_details_load_a_build_by_its_number() {
        let (plugin, requests) = plugin_answering(build_150, "acme/app").await;

        let run = plugin
            .fetch_run_details("buildkite__1__acme__app", 150)
            .await
            .unwrap();

        assert_eq!(run.run_number, 150);
        assert_eq!(run.status, PipelineStatus::Failed);
        assert_eq!(run.duration_seconds, Some(120));
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn build_jobs_become_log_nodes() {
        let (plugin, _) = plugin_answering(build_150, "acme/app").await;
        let metadata = HashMap::new();

        let nodes = plugin.fetch_log_nodes(run_150(&metadata)).await.unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, JOB_ID);
        assert_eq!(nodes[0].name, "Tests");
        assert_eq!(nodes[0].status, PipelineStatus::Failed);
        assert_eq!(nodes[0].log, Some(LogAvailability::Ready));
    }

    #[tokio::test]
    async fn job_log_streams_the_raw_text() {
        let (plugin, _) = plugin_answering(build_150, "acme/app").await;

        assert_eq!(read(&plugin, JOB_ID).await.unwrap(), RAW_LOG.as_bytes());
    }

    #[tokio::test]
    async fn missing_job_log_is_not_found() {
        let (plugin, _) = plugin_answering(build_150, "acme/app").await;

        let error = read(&plugin, "0190d2a4-0000-0000-0000-000000000000")
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn job_ids_outside_the_uuid_alphabet_are_rejected_without_a_request() {
        let (plugin, requests) = plugin_answering(build_150, "acme/app").await;

        for node_id in ["", "../../builds/150", "0190d2a4/log", "x?y=1"] {
            let error = read(&plugin, node_id).await.unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{node_id}: {error:?}"
            );
        }
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn transient_failure_on_any_pipeline_fails_the_fetch() {
        let plugin = plugin_selecting("acme/ok,acme/limited").await;

        let error = plugin.fetch_pipelines().await.unwrap_err();

        assert!(
            matches!(error, PluginError::RateLimited { .. }),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn permanent_failure_on_one_pipeline_keeps_the_rest() {
        let plugin = plugin_selecting("acme/ok,acme/missing").await;

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        assert_eq!(pipelines.len(), 1);
    }

    #[derive(Default)]
    struct JobLog {
        body: Vec<u8>,
        grow_after_head: Vec<u8>,
        min_suffix: usize,
        limited: bool,
        requests: Vec<(String, Option<String>)>,
    }

    type SharedJobLog = Arc<std::sync::Mutex<JobLog>>;

    fn job_log(body: &[u8]) -> SharedJobLog {
        Arc::new(std::sync::Mutex::new(JobLog {
            body: body.to_vec(),
            ..JobLog::default()
        }))
    }

    fn job_log_target(method: &str) -> String {
        format!("{method} {BUILD_PATH}/jobs/{JOB_ID}/log")
    }

    async fn serve_job_log(log: &SharedJobLog) -> BuildkitePlugin {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = log.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0; 8192];
                let count = tokio::io::AsyncReadExt::read(&mut stream, &mut buffer)
                    .await
                    .unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let mut request_line = request.lines().next().unwrap().split_whitespace();
                let method = request_line.next().unwrap().to_string();
                let target = request_line.next().unwrap().to_string();
                let range = request
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("range"))
                    .map(|(_, value)| value.trim().to_string());
                let (status, headers, payload, length) = {
                    let mut log = state.lock().unwrap();
                    log.requests
                        .push((format!("{method} {target}"), range.clone()));
                    let total = log.body.len();
                    let suffix = range
                        .as_deref()
                        .and_then(|value| value.strip_prefix("bytes=-")?.parse::<usize>().ok());
                    if log.limited {
                        (
                            "429 Too Many Requests",
                            "Retry-After: 120\r\n".to_string(),
                            Vec::new(),
                            0,
                        )
                    } else if method == "HEAD" {
                        let grown = std::mem::take(&mut log.grow_after_head);
                        log.body.extend_from_slice(&grown);
                        ("200 OK", String::new(), Vec::new(), total)
                    } else if let Some(suffix) = suffix {
                        let start = total - suffix.max(log.min_suffix).min(total);
                        let payload = log.body[start..].to_vec();
                        let length = payload.len();
                        (
                            "206 Partial Content",
                            format!("Content-Range: bytes {start}-{}/{total}\r\n", total - 1),
                            payload,
                            length,
                        )
                    } else {
                        ("200 OK", String::new(), log.body.clone(), total)
                    }
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\n{headers}Content-Length: {length}\r\nConnection: close\r\n\r\n"
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, head.as_bytes()).await;
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, &payload).await;
            }
        });
        let mut plugin = BuildkitePlugin::new();
        plugin.client = Some(
            client::BuildkiteClient::new(Arc::new(reqwest::Client::new()), "token".to_string())
                .with_base_url(format!("http://{addr}")),
        );
        plugin
    }

    fn cursor(job: &str, offset: u64) -> LogCursor {
        LogCursor::new(&serde_json::json!({"job": job, "offset": offset})).unwrap()
    }

    async fn read_job_log(
        plugin: &BuildkitePlugin, from: Option<&LogCursor>,
    ) -> (PluginResult<LogRead>, Vec<u8>, bool) {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_150(&metadata), JOB_ID, from, &mut sink)
            .await;
        let restarted = sink.restarted();
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        (result, bytes, restarted)
    }

    fn requests_of(log: &SharedJobLog) -> Vec<(String, Option<String>)> {
        log.lock().unwrap().requests.clone()
    }

    #[tokio::test]
    async fn resumed_job_log_fetches_only_the_unseen_suffix() {
        let log = job_log(b"a\nb\n");
        let plugin = serve_job_log(&log).await;

        let (first, bytes, restarted) = read_job_log(&plugin, None).await;
        assert_eq!(first.unwrap(), LogRead::replace(Some(cursor(JOB_ID, 4))));
        assert_eq!(bytes, b"a\nb\n");
        assert!(!restarted);

        log.lock().unwrap().body.extend_from_slice(b"c\nd\n");
        let (second, bytes, restarted) = read_job_log(&plugin, Some(&cursor(JOB_ID, 4))).await;
        assert_eq!(second.unwrap(), LogRead::append(cursor(JOB_ID, 8)));
        assert_eq!(bytes, b"c\nd\n");
        assert!(!restarted);

        let (idle, bytes, restarted) = read_job_log(&plugin, Some(&cursor(JOB_ID, 8))).await;
        assert_eq!(idle.unwrap(), LogRead::append(cursor(JOB_ID, 8)));
        assert!(bytes.is_empty() && !restarted);
        assert_eq!(
            requests_of(&log),
            [
                (job_log_target("GET"), None),
                (job_log_target("HEAD"), None),
                (job_log_target("GET"), Some("bytes=-4".to_string())),
                (job_log_target("HEAD"), None),
            ]
        );
    }

    #[tokio::test]
    async fn growth_between_the_size_check_and_the_fetch_loses_and_repeats_nothing() {
        let log = job_log(b"a\nb\nx\n");
        log.lock().unwrap().grow_after_head = b"y\n".to_vec();
        let plugin = serve_job_log(&log).await;

        let (read, bytes, restarted) = read_job_log(&plugin, Some(&cursor(JOB_ID, 4))).await;

        assert_eq!(read.unwrap(), LogRead::append(cursor(JOB_ID, 8)));
        assert_eq!(bytes, b"x\ny\n");
        assert!(!restarted);
        assert_eq!(
            requests_of(&log),
            [
                (job_log_target("HEAD"), None),
                (job_log_target("GET"), Some("bytes=-2".to_string())),
                (job_log_target("GET"), Some("bytes=-4".to_string())),
            ]
        );
    }

    #[tokio::test]
    async fn suffix_overlapping_the_cursor_is_dropped_before_writing() {
        let log = job_log(b"a\nb\nc\n");
        log.lock().unwrap().min_suffix = 5;
        let plugin = serve_job_log(&log).await;

        let (read, bytes, restarted) = read_job_log(&plugin, Some(&cursor(JOB_ID, 4))).await;

        assert_eq!(read.unwrap(), LogRead::append(cursor(JOB_ID, 6)));
        assert_eq!(bytes, b"c\n");
        assert!(!restarted);
    }

    #[tokio::test]
    async fn shrunk_job_log_restarts_with_the_whole_log() {
        let log = job_log(b"a\n");
        let plugin = serve_job_log(&log).await;

        let (read, bytes, restarted) = read_job_log(&plugin, Some(&cursor(JOB_ID, 6))).await;

        assert_eq!(read.unwrap(), LogRead::replace(Some(cursor(JOB_ID, 2))));
        assert_eq!(bytes, b"a\n");
        assert!(restarted);
        assert_eq!(
            requests_of(&log),
            [
                (job_log_target("HEAD"), None),
                (job_log_target("GET"), None)
            ]
        );
    }

    #[tokio::test]
    async fn unusable_cursors_restart_without_a_size_check() {
        let log = job_log(b"a\nb\n");
        let plugin = serve_job_log(&log).await;

        for stale in [
            LogCursor::new(&"garbage").unwrap(),
            cursor("0190d2a4-0000-0000-0000-000000000000", 4),
        ] {
            let (read, bytes, restarted) = read_job_log(&plugin, Some(&stale)).await;
            assert_eq!(read.unwrap(), LogRead::replace(Some(cursor(JOB_ID, 4))));
            assert_eq!(bytes, b"a\nb\n");
            assert!(restarted);
        }

        assert_eq!(
            requests_of(&log),
            [(job_log_target("GET"), None), (job_log_target("GET"), None)]
        );
    }

    #[tokio::test]
    async fn rate_limited_resume_reports_the_delay_and_stops_after_one_request() {
        let log = job_log(b"a\nb\n");
        log.lock().unwrap().limited = true;
        let plugin = serve_job_log(&log).await;

        let (read, bytes, restarted) = read_job_log(&plugin, Some(&cursor(JOB_ID, 2))).await;

        let error = read.unwrap_err();
        assert!(
            matches!(error, PluginError::RateLimited { .. }),
            "{error:?}"
        );
        assert_eq!(error.retry_after(), Some(Duration::from_secs(120)));
        assert!(bytes.is_empty() && !restarted);
        assert_eq!(requests_of(&log), [(job_log_target("HEAD"), None)]);
    }
}
