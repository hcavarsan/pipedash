use std::collections::{
    BTreeSet,
    HashMap,
};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use futures::future::try_join_all;
use pipedash_plugin_api::*;

use crate::log_dialect::WoodpeckerLogDialect;
use crate::mapper::BRANCH_INPUT;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

const LOG_CHUNK_BYTES: usize = 64 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    step: String,
    lines: usize,
}

pub struct WoodpeckerPlugin {
    metadata: PluginMetadata,
    client: Option<client::WoodpeckerClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    status_source: Option<StatusSource>,
}

impl Default for WoodpeckerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl WoodpeckerPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            status_source: None,
        }
    }

    fn client(&self) -> PluginResult<&client::WoodpeckerClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn provider_id(&self) -> PluginResult<i64> {
        self.provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))
    }

    async fn fetch_repo_pipeline(
        &self, provider_id: i64, full_name: &str,
    ) -> PluginResult<Pipeline> {
        let client = self.client()?;
        let repo = client.lookup_repo(full_name).await?;
        let latest = client.list_pipelines(repo.id, 1).await?;
        Ok(mapper::map_pipeline(provider_id, &repo, latest.first()))
    }

    async fn write_entries(entries: &[types::LogEntry], sink: &mut LogSink) -> PluginResult<()> {
        let mut chunk = Vec::new();
        for entry in entries {
            if let Some(data) = &entry.data {
                base64::engine::general_purpose::STANDARD
                    .decode_vec(data, &mut chunk)
                    .map_err(|e| {
                        PluginError::SerializationError(format!("Invalid Woodpecker log line: {e}"))
                    })?;
            }
            chunk.push(b'\n');
            if chunk.len() >= LOG_CHUNK_BYTES {
                sink.write(std::mem::take(&mut chunk).into()).await?;
            }
        }
        sink.write(chunk.into()).await
    }
}

#[async_trait]
impl Plugin for WoodpeckerPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let base_url = config::get_base_url(&config)?;
        let token = config
            .get("token")
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                PluginError::InvalidConfig("Missing Woodpecker personal access token".to_string())
            })?
            .to_string();
        let status_source = custom_status_source(&config)?;

        let http_client = match http_client {
            Some(http_client) => http_client,
            None => Arc::new(
                reqwest::Client::builder()
                    .use_rustls_tls()
                    .https_only(cfg!(target_os = "ios"))
                    .pool_max_idle_per_host(10)
                    .timeout(Duration::from_secs(30))
                    .connect_timeout(Duration::from_secs(10))
                    .tcp_keepalive(Duration::from_secs(60))
                    .build()
                    .map_err(|e| {
                        PluginError::Internal(format!("Failed to build HTTP client: {e}"))
                    })?,
            ),
        };

        self.status_source = status_source;
        self.client = Some(client::WoodpeckerClient::new(http_client, base_url, token));
        self.provider_id = Some(provider_id);
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.client()?.ping().await?;
        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedAvailablePipelines> {
        self.fetch_available_pipelines_filtered(None, None, params)
            .await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let repos = self.client()?.list_repos().await?;
        let owners: BTreeSet<&str> = repos.iter().map(|repo| repo.owner.as_str()).collect();
        Ok(owners
            .into_iter()
            .map(|owner| Organization {
                id: owner.to_string(),
                name: owner.to_string(),
                description: None,
            })
            .collect())
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedAvailablePipelines> {
        let client = self.client()?;
        let params = params.unwrap_or_default();
        let start = params
            .calculate_offset()
            .map_err(PluginError::InvalidConfig)?;

        let term = search.as_deref().map(str::to_lowercase);
        let mut repos = client.list_repos().await?;
        repos.retain(|repo| {
            org.as_deref().is_none_or(|org| repo.owner == org)
                && term
                    .as_deref()
                    .is_none_or(|term| repo.full_name.to_lowercase().contains(term))
        });

        let total_count = repos.len();
        let items = repos
            .iter()
            .skip(start)
            .take(params.page_size)
            .map(mapper::map_available_pipeline)
            .collect::<Vec<_>>();
        let mut response =
            PaginatedResponse::new(items, params.page, params.page_size, total_count);
        response.has_more = start + response.items.len() < total_count;
        Ok(response)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self.provider_id()?;
        let repositories = config::get_repositories(&self.config);
        if repositories.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No repositories configured".to_string(),
            ));
        }

        try_join_all(
            repositories
                .iter()
                .map(|full_name| self.fetch_repo_pipeline(provider_id, full_name)),
        )
        .await
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let repo_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let runs = client.list_pipelines(repo_id, limit).await?;
        Ok(runs
            .iter()
            .map(|run| {
                mapper::map_run(
                    run,
                    pipeline_id,
                    config::pipeline_web_url(client.base_url(), repo_id, run.number),
                )
            })
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let repo_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let run = client.get_pipeline(repo_id, run_number).await?;
        Ok(mapper::map_run(
            &run,
            pipeline_id,
            config::pipeline_web_url(client.base_url(), repo_id, run.number),
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let repo_id = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let pipeline = client.get_pipeline(repo_id, run.run_number).await?;
        Ok(mapper::workflows_to_log_nodes(
            &pipeline,
            &config::pipeline_web_url(client.base_url(), repo_id, run.run_number),
        ))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        if node_id.is_empty() || !node_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid Woodpecker step ID: {node_id}"
            )));
        }
        let repo_id = config::parse_pipeline_id(run.pipeline_id)?;
        let entries = self
            .client()?
            .step_logs(repo_id, run.run_number, node_id)
            .await?;

        let resumed = from
            .and_then(|cursor| cursor.parse::<LogPosition>())
            .filter(|position| position.step == node_id && position.lines <= entries.len());
        if from.is_some() && resumed.is_none() {
            sink.restart()?;
        }
        Self::write_entries(&entries[resumed.as_ref().map_or(0, |p| p.lines)..], sink).await?;

        let cursor = LogCursor::new(&LogPosition {
            step: node_id.to_string(),
            lines: entries.len(),
        })?;
        Ok(match resumed {
            Some(_) => LogRead::append(cursor),
            None => LogRead::replace(Some(cursor)),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &WoodpeckerLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let repo_id = config::parse_pipeline_id(workflow_id)?;
        let repo = self.client()?.get_repo(repo_id).await?;
        Ok(vec![WorkflowParameter {
            name: BRANCH_INPUT.to_string(),
            label: Some("Branch".to_string()),
            description: Some("Branch to run the pipeline on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some(repo.default_branch).filter(|branch| !branch.is_empty()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let repo_id = config::parse_pipeline_id(&params.workflow_id)?;
        let branch = params
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get(BRANCH_INPUT))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|branch| !branch.is_empty())
            .ok_or_else(|| {
                PluginError::InvalidConfig(
                    "Branch is required to start a Woodpecker pipeline".to_string(),
                )
            })?;
        let client = self.client()?;
        let pipeline = client
            .create_pipeline(repo_id, branch)
            .await?
            .ok_or_else(|| {
                PluginError::ApiError(format!(
                    "Woodpecker started no pipeline on {branch}: no workflow runs on manual events"
                ))
            })?;
        Ok(serde_json::json!({
            "message": format!("Started pipeline #{} on {branch}", pipeline.number),
            "run_number": pipeline.number,
            "logs_url": config::pipeline_web_url(client.base_url(), repo_id, pipeline.number)
        })
        .to_string())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        self.client()?
            .cancel_pipeline(config::parse_pipeline_id(pipeline_id)?, run_number)
            .await
    }
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
    use std::net::SocketAddr;
    use std::sync::{
        Mutex,
        MutexGuard,
    };

    use serde_json::{
        json,
        Value,
    };
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::net::{
        TcpListener,
        TcpStream,
    };

    use super::*;

    struct Request {
        method: String,
        url: reqwest::Url,
        authorization: Option<String>,
        body: String,
    }

    impl Request {
        fn query(&self, key: &str) -> Option<String> {
            self.url
                .query_pairs()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.into_owned())
        }
    }

    struct Reply {
        status: u16,
        body: String,
    }

    impl Reply {
        fn json(status: u16, body: Value) -> Self {
            Self {
                status,
                body: body.to_string(),
            }
        }

        fn empty(status: u16) -> Self {
            Self {
                status,
                body: String::new(),
            }
        }

        fn not_found(request: &Request) -> Self {
            Self::json(
                404,
                json!({"message": format!("unexpected {} {}", request.method, request.url)}),
            )
        }

        fn to_http(&self) -> String {
            format!(
                "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                self.status,
                self.body.len(),
                self.body
            )
        }
    }

    struct Stub {
        base_url: String,
        requests: Arc<Mutex<Vec<Request>>>,
        server: tokio::task::JoinHandle<()>,
    }

    impl Stub {
        fn requests(&self) -> MutexGuard<'_, Vec<Request>> {
            self.requests.lock().unwrap()
        }

        fn requests_to(&self, method: &str, path: &str) -> Vec<Option<String>> {
            self.requests()
                .iter()
                .filter(|request| request.method == method && request.url.path() == path)
                .map(|request| request.query("perPage"))
                .collect()
        }
    }

    impl Drop for Stub {
        fn drop(&mut self) {
            self.server.abort();
        }
    }

    async fn read_request(stream: &mut TcpStream, addr: SocketAddr) -> Request {
        let mut raw = Vec::new();
        let mut buffer = [0; 4096];
        let header_end = loop {
            if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
            let count = stream.read(&mut buffer).await.unwrap();
            assert!(count > 0, "connection closed before the request head ended");
            raw.extend_from_slice(&buffer[..count]);
        };
        let head = String::from_utf8(raw[..header_end].to_vec()).unwrap();
        let header = |name: &str| {
            head.lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.trim().to_string())
        };
        let length: usize = header("content-length")
            .map(|value| value.parse().unwrap())
            .unwrap_or(0);
        while raw.len() < header_end + length {
            let count = stream.read(&mut buffer).await.unwrap();
            assert!(count > 0, "connection closed before the request body ended");
            raw.extend_from_slice(&buffer[..count]);
        }

        let mut request_line = head.lines().next().unwrap().split_whitespace();
        let method = request_line.next().unwrap().to_string();
        let target = request_line.next().unwrap();
        Request {
            method,
            url: reqwest::Url::parse(&format!("http://{addr}{target}")).unwrap(),
            authorization: header("authorization"),
            body: String::from_utf8(raw[header_end..header_end + length].to_vec()).unwrap(),
        }
    }

    async fn serve(reply: fn(&Request) -> Reply) -> Stub {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream, addr).await;
                let answer = reply(&request);
                recorded.lock().unwrap().push(request);
                stream.write_all(answer.to_http().as_bytes()).await.unwrap();
            }
        });
        Stub {
            base_url: format!("http://{addr}"),
            requests,
            server,
        }
    }

    fn plugin_for(stub: &Stub, selected: &str) -> WoodpeckerPlugin {
        let mut plugin = WoodpeckerPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("base_url".to_string(), stub.base_url.clone()),
                    ("token".to_string(), "stub-token".to_string()),
                    ("selected_items".to_string(), selected.to_string()),
                ]),
                None,
            )
            .unwrap();
        plugin
    }

    fn repo_json(id: i64, owner: &str, name: &str) -> Value {
        json!({
            "id": id,
            "owner": owner,
            "name": name,
            "full_name": format!("{owner}/{name}"),
            "default_branch": "trunk",
            "config_file": ".woodpecker.yaml"
        })
    }

    fn pipeline_json(number: i64, status: &str) -> Value {
        json!({
            "id": 1000 + number,
            "number": number,
            "status": status,
            "event": "push",
            "branch": "main",
            "commit": format!("sha{number}"),
            "message": "change",
            "author": "dev",
            "created": 1_761_645_600 + number,
            "started": 1_761_645_610 + number,
            "finished": 1_761_645_670 + number
        })
    }

    fn capped_history(request: &Request) -> Reply {
        if request.url.path() != "/api/repos/5/pipelines" {
            return Reply::not_found(request);
        }
        let page: i64 = request.query("page").unwrap().parse().unwrap();
        let size = request
            .query("perPage")
            .unwrap()
            .parse::<i64>()
            .unwrap()
            .min(20);
        let first = 45 - (page - 1) * size;
        let runs: Vec<Value> = (1..=first)
            .rev()
            .take(size as usize)
            .map(|number| pipeline_json(number, "success"))
            .collect();
        Reply::json(200, json!(runs))
    }

    fn user_repos(request: &Request) -> Reply {
        match request.url.path() {
            "/api/user/repos" => Reply::json(
                200,
                json!([
                    repo_json(1, "acme", "api"),
                    repo_json(2, "acme", "web"),
                    repo_json(3, "acme", "website"),
                    repo_json(4, "other", "web"),
                ]),
            ),
            _ => Reply::not_found(request),
        }
    }

    fn trigger_params(inputs: Option<Value>) -> TriggerParams {
        TriggerParams {
            workflow_id: "woodpecker__1__5".to_string(),
            inputs,
        }
    }

    #[tokio::test]
    async fn pipelines_follow_the_latest_pipeline_of_each_selected_repository() {
        let stub = serve(|request| match request.url.path() {
            "/api/repos/lookup/acme/web" => Reply::json(200, repo_json(5, "acme", "web")),
            "/api/repos/lookup/acme/empty" => Reply::json(200, repo_json(6, "acme", "empty")),
            "/api/repos/5/pipelines" => Reply::json(200, json!([pipeline_json(12, "failure")])),
            "/api/repos/6/pipelines" => Reply::json(200, json!([])),
            _ => Reply::not_found(request),
        })
        .await;
        let plugin = plugin_for(&stub, "acme/web,acme/empty");

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        let summary: Vec<_> = pipelines
            .iter()
            .map(|p| {
                (
                    p.id.as_str(),
                    p.name.as_str(),
                    p.status,
                    p.branch.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "woodpecker__1__5",
                    "web",
                    PipelineStatus::Failed,
                    Some("main")
                ),
                ("woodpecker__1__6", "empty", PipelineStatus::Pending, None),
            ]
        );
        assert_eq!(
            stub.requests_to("GET", "/api/repos/5/pipelines"),
            [Some("1".to_string())]
        );
        assert_eq!(
            stub.requests_to("GET", "/api/repos/6/pipelines"),
            [Some("1".to_string())]
        );
    }

    #[tokio::test]
    async fn run_history_reads_past_a_server_page_cap_until_an_empty_page() {
        let stub = serve(capped_history).await;
        let plugin = plugin_for(&stub, "acme/web");

        let runs = plugin
            .fetch_run_history("woodpecker__1__5", 100)
            .await
            .unwrap();

        let numbers: Vec<i64> = runs.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, (1..=45).rev().collect::<Vec<_>>());
        assert_eq!(stub.requests_to("GET", "/api/repos/5/pipelines").len(), 4);
        assert_eq!(
            runs[0].logs_url,
            format!("{}/repos/5/pipeline/45", stub.base_url)
        );
    }

    #[tokio::test]
    async fn run_history_stops_once_the_limit_is_reached() {
        let stub = serve(capped_history).await;
        let plugin = plugin_for(&stub, "acme/web");

        let runs = plugin
            .fetch_run_history("woodpecker__1__5", 30)
            .await
            .unwrap();

        let numbers: Vec<i64> = runs.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, (16..=45).rev().collect::<Vec<_>>());
        assert_eq!(stub.requests_to("GET", "/api/repos/5/pipelines").len(), 2);
    }

    #[tokio::test]
    async fn available_repositories_filter_by_owner_and_search_then_paginate() {
        let stub = serve(user_repos).await;
        let plugin = plugin_for(&stub, "");

        let response = plugin
            .fetch_available_pipelines_filtered(
                Some("acme".into()),
                Some("WEB".into()),
                Some(PaginationParams {
                    page: 2,
                    page_size: 1,
                }),
            )
            .await
            .unwrap();

        let ids: Vec<&str> = response.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["acme/website"]);
        assert_eq!(response.total_count, 2);
        assert!(!response.has_more);
    }

    #[tokio::test]
    async fn organizations_are_the_distinct_repository_owners() {
        let stub = serve(user_repos).await;
        let plugin = plugin_for(&stub, "");

        let organizations = plugin.fetch_organizations().await.unwrap();

        let ids: Vec<&str> = organizations.iter().map(|org| org.id.as_str()).collect();
        assert_eq!(ids, ["acme", "other"]);
    }

    #[tokio::test]
    async fn trigger_starts_a_manual_pipeline_on_the_requested_branch() {
        let stub = serve(
            |request| match (request.method.as_str(), request.url.path()) {
                ("POST", "/api/repos/5/pipelines") => {
                    Reply::json(200, pipeline_json(13, "pending"))
                }
                _ => Reply::not_found(request),
            },
        )
        .await;
        let plugin = plugin_for(&stub, "acme/web");

        let result = plugin
            .trigger_pipeline(trigger_params(Some(json!({"branch": " release "}))))
            .await
            .unwrap();

        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result["run_number"], 13);
        assert_eq!(
            result["logs_url"],
            format!("{}/repos/5/pipeline/13", stub.base_url)
        );
        let requests = stub.requests();
        assert_eq!(
            serde_json::from_str::<Value>(&requests[0].body).unwrap(),
            json!({"branch": "release"})
        );
        assert_eq!(
            requests[0].authorization.as_deref(),
            Some("Bearer stub-token")
        );
    }

    #[tokio::test]
    async fn trigger_fails_when_woodpecker_filters_the_pipeline() {
        let stub = serve(|_| Reply::empty(204)).await;
        let plugin = plugin_for(&stub, "acme/web");

        let result = plugin
            .trigger_pipeline(trigger_params(Some(json!({"branch": "main"}))))
            .await;

        assert!(
            matches!(result, Err(PluginError::ApiError(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn trigger_requires_a_branch_and_sends_nothing() {
        let stub = serve(Reply::not_found).await;
        let plugin = plugin_for(&stub, "acme/web");

        let result = plugin.trigger_pipeline(trigger_params(None)).await;

        assert!(
            matches!(result, Err(PluginError::InvalidConfig(_))),
            "{result:?}"
        );
        assert!(stub.requests().is_empty());
    }

    #[tokio::test]
    async fn workflow_parameters_default_to_the_repository_branch() {
        let stub = serve(|request| match request.url.path() {
            "/api/repos/5" => Reply::json(200, repo_json(5, "acme", "web")),
            _ => Reply::not_found(request),
        })
        .await;
        let plugin = plugin_for(&stub, "acme/web");

        let parameters = plugin
            .fetch_workflow_parameters("woodpecker__1__5")
            .await
            .unwrap();

        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, BRANCH_INPUT);
        assert!(parameters[0].required);
        assert!(matches!(
            &parameters[0].param_type,
            WorkflowParameterType::String { default: Some(branch) } if branch == "trunk"
        ));
    }

    fn step_logs(request: &Request) -> Reply {
        if request.url.path() != "/api/repos/5/logs/7/77" {
            return Reply::not_found(request);
        }
        let entries: Vec<Value> = ["▶  make", "ok", "done"]
            .iter()
            .enumerate()
            .map(|(line, text)| {
                json!({
                    "id": line,
                    "step_id": 77,
                    "line": line,
                    "time": line,
                    "type": 0,
                    "data": base64::engine::general_purpose::STANDARD.encode(text)
                })
            })
            .collect();
        Reply::json(200, json!(entries))
    }

    fn cursor(step: &str, lines: usize) -> LogCursor {
        LogCursor::new(&json!({"step": step, "lines": lines})).unwrap()
    }

    async fn read_log(
        plugin: &WoodpeckerPlugin, node_id: &str, from: Option<&LogCursor>,
    ) -> (PluginResult<LogRead>, Vec<u8>, bool) {
        let metadata = HashMap::new();
        let run = RunRef {
            pipeline_id: "woodpecker__1__5",
            run_number: 7,
            run_id: "woodpecker-run-1007",
            metadata: &metadata,
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin.read_log(run, node_id, from, &mut sink).await;
        let restarted = sink.restarted();
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        (result, bytes, restarted)
    }

    #[tokio::test]
    async fn step_logs_decode_and_resume_after_the_lines_already_read() {
        let stub = serve(step_logs).await;
        let plugin = plugin_for(&stub, "acme/web");
        let all = "▶  make\nok\ndone\n".as_bytes();

        let (result, bytes, restarted) = read_log(&plugin, "77", None).await;
        assert_eq!(result.unwrap(), LogRead::replace(Some(cursor("77", 3))));
        assert_eq!(bytes, all);
        assert!(!restarted);

        let (result, bytes, restarted) = read_log(&plugin, "77", Some(&cursor("77", 2))).await;
        assert_eq!(result.unwrap(), LogRead::append(cursor("77", 3)));
        assert_eq!(bytes, b"done\n");
        assert!(!restarted);

        for stale in [cursor("77", 5), cursor("78", 1)] {
            let (result, bytes, restarted) = read_log(&plugin, "77", Some(&stale)).await;
            assert_eq!(result.unwrap(), LogRead::replace(Some(cursor("77", 3))));
            assert_eq!(bytes, all);
            assert!(restarted);
        }
    }
}
