use std::collections::{
    BTreeSet,
    HashMap,
};
use std::sync::Arc;

use async_trait::async_trait;
use pipedash_plugin_api::*;
use tracing::{
    debug,
    info,
    warn,
};

use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

const DEFAULT_PAGE_SIZE: usize = 100;

pub struct KomodoPlugin {
    metadata: PluginMetadata,
    client: Option<client::KomodoClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    server_url: String,
}

impl Default for KomodoPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl KomodoPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            server_url: String::new(),
        }
    }

    fn client(&self) -> PluginResult<&client::KomodoClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn provider_id(&self) -> PluginResult<i64> {
        self.provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))
    }
}

#[async_trait]
impl Plugin for KomodoPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "komodo"
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions {
            origin: config::get_server_url(config)
                .ok()
                .and_then(|url| reqwest::Url::parse(&url).ok()),
            accept_invalid_certs: config::is_insecure(config),
        }
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing Komodo plugin");

        let server_url = config::get_server_url(&config)?;
        let api_key = config::get_credential(&config, "api_key")?;
        let api_secret = config::get_credential(&config, "token")?;
        if config::is_insecure(&config) {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }
        let http_client = http_client.ok_or_else(|| {
            PluginError::Internal("Komodo plugin requires an HTTP client".to_string())
        })?;

        self.client = Some(client::KomodoClient::new(
            http_client,
            server_url.clone(),
            api_key,
            api_secret,
        ));
        self.provider_id = Some(provider_id);
        self.server_url = server_url;
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let version = self.client()?.version().await?;
        debug!(version, "Komodo credentials validated");
        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        self.fetch_available_pipelines_filtered(None, None, params)
            .await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let stacks = self.client()?.list_stacks().await?;
        let hosts: BTreeSet<&str> = stacks
            .iter()
            .map(|stack| stack.info.host())
            .filter(|host| !host.is_empty())
            .collect();

        Ok(hosts
            .into_iter()
            .map(|host| Organization {
                id: host.to_string(),
                name: host.to_string(),
                description: None,
            })
            .collect())
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let search = search.map(|term| term.to_lowercase());
        let matching: Vec<AvailablePipeline> = self
            .client()?
            .list_stacks()
            .await?
            .iter()
            .map(mapper::map_available_pipeline)
            .filter(|stack| org.is_none() || stack.organization == org)
            .filter(|stack| {
                search.as_ref().is_none_or(|term| {
                    stack.name.to_lowercase().contains(term)
                        || stack
                            .description
                            .as_ref()
                            .is_some_and(|text| text.to_lowercase().contains(term))
                })
            })
            .collect();

        let total_count = matching.len();
        let params = params.unwrap_or_default();
        let page = params.page.max(1);
        let page_size = if params.page_size == 0 {
            DEFAULT_PAGE_SIZE
        } else {
            params.page_size
        };
        let offset = (page - 1).saturating_mul(page_size);
        let items: Vec<AvailablePipeline> =
            matching.into_iter().skip(offset).take(page_size).collect();

        let has_more = offset + items.len() < total_count;
        let mut response = PaginatedResponse::new(items, page, page_size, total_count);
        response.has_more = has_more;
        Ok(response)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self.provider_id()?;
        let selected = config::parse_selected_items(&self.config);
        let client = self.client()?;
        let stacks = client.list_stacks().await?;

        let listed: Vec<&types::StackListItem> = stacks
            .iter()
            .filter(|stack| {
                selected.as_ref().is_none_or(|items| {
                    items
                        .iter()
                        .any(|item| *item == stack.id || *item == stack.name)
                })
            })
            .collect();
        let stack_ids: Vec<&str> = listed.iter().map(|stack| stack.id.as_str()).collect();
        let last_starts = client.latest_update_starts(&stack_ids).await?;

        let pipelines: Vec<Pipeline> = listed
            .iter()
            .map(|stack| {
                mapper::map_pipeline(stack, provider_id, last_starts.get(&stack.id).copied())
            })
            .collect();

        debug!(
            stacks = stacks.len(),
            pipelines = pipelines.len(),
            "Mapped Komodo stacks to pipelines"
        );
        Ok(pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (_, stack_id) = config::parse_pipeline_id(pipeline_id)?;
        let updates = self.client()?.stack_updates(stack_id, limit).await?;

        Ok(updates
            .iter()
            .map(|update| mapper::map_run(pipeline_id, stack_id, &self.server_url, update, None))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (_, stack_id) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let mut matching = client.updates_started_at(stack_id, run_number).await?;
        if matching.len() > 1 {
            return Err(PluginError::ApiError(format!(
                "{} updates of stack {stack_id} share run number {run_number}; refusing to pick one",
                matching.len()
            )));
        }
        let item = matching.pop().ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "Update {run_number} not found for stack {stack_id}"
            ))
        })?;
        let detail = if item.id.is_empty() {
            None
        } else {
            Some(client.update(&item.id).await?)
        };

        Ok(mapper::map_run(
            pipeline_id,
            stack_id,
            &self.server_url,
            &item,
            detail.as_ref(),
        ))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_, stack_id) = config::parse_pipeline_id(&params.workflow_id)?;
        let update = self.client()?.deploy_stack(stack_id).await?;
        let update_id = update
            .id
            .map(types::MongoId::into_string)
            .unwrap_or_default();
        info!(stack_id, update_id, "Triggered Komodo stack deploy");

        Ok(serde_json::json!({
            "message": format!("Triggered deploy of stack {stack_id}"),
            "run_number": update.start_ts,
            "logs_url": mapper::run_url(&self.server_url, stack_id, &update_id),
        })
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::{
        json,
        Value,
    };
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::sync::Mutex;

    use super::*;

    const API_KEY: &str = "K-stub";
    const API_SECRET: &str = "S-stub";
    const PIPELINE: &str = "komodo__1__stk1";
    const NEWEST_START: i64 = 1_761_660_000_000;
    const UPDATE_COUNT: usize = 250;
    const UPDATES_PER_PAGE: usize = 100;
    const BUSY_START: i64 = NEWEST_START - 5_000_000;
    const BUSY_UPDATES: usize = 650;
    const RUNNING_START: i64 = NEWEST_START - 10_000_000;
    const RUNNING_UPDATES: usize = 150;
    const DOWN_START: i64 = NEWEST_START - 20_000_000;
    const STATES: [&str; 11] = [
        "deploying",
        "running",
        "paused",
        "stopped",
        "created",
        "restarting",
        "dead",
        "removing",
        "unhealthy",
        "down",
        "unknown",
    ];

    struct Recorded {
        path: String,
        headers: HashMap<String, String>,
        body: Value,
    }

    type Requests = Arc<Mutex<Vec<Recorded>>>;

    fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn stacks() -> Value {
        let mut stacks: Vec<Value> = STATES
            .iter()
            .map(|state| {
                json!({
                    "id": format!("id-{state}"), "type": "Stack", "name": format!("stack-{state}"),
                    "template": false, "tags": [],
                    "info": {
                        "server_id": "srv1", "server_name": "prod-1", "repo": "acme/web",
                        "branch": "main", "repo_link": "https://github.com/acme/web",
                        "state": state, "status": "up",
                        "services": [{"service": "app", "image": "web", "update_available": *state == "running"}],
                        "project_missing": false, "missing_files": []
                    }
                })
            })
            .collect();
        stacks.push(json!({
            "id": "id-template", "type": "Stack", "name": "stack-template", "template": true,
            "tags": [], "info": {"state": "running"}
        }));
        Value::Array(stacks)
    }

    fn update_start(index: usize) -> i64 {
        NEWEST_START - index as i64 * 1_000
    }

    fn listed_update(index: usize) -> Value {
        json!({
            "id": format!("u{index:03}"),
            "operation": if index % 2 == 0 { "DeployStack" } else { "StartStack" },
            "start_ts": update_start(index), "success": index % 5 != 0,
            "username": "ada", "operator": "usr1", "target": {"type": "Stack", "id": "stk1"},
            "status": if index == 0 { "InProgress" } else { "Complete" }
        })
    }

    fn full_update(index: usize) -> Value {
        json!({
            "_id": {"$oid": format!("u{index:03}")},
            "operation": if index % 2 == 0 { "DeployStack" } else { "StartStack" },
            "start_ts": update_start(index), "success": index % 5 != 0, "operator": "usr1",
            "target": {"type": "Stack", "id": "stk1"}, "logs": [],
            "end_ts": if index == 0 { Value::Null } else { json!(update_start(index) + 42_500) },
            "status": if index == 0 { "InProgress" } else { "Complete" },
            "commit_hash": "abc1234"
        })
    }

    fn stack_update(stack_id: &str, index: usize, start_ts: i64) -> Value {
        json!({
            "id": format!("{stack_id}-u{index}"), "operation": "DeployStack", "start_ts": start_ts,
            "success": true, "username": "ada", "operator": "usr1",
            "target": {"type": "Stack", "id": stack_id}, "status": "Complete"
        })
    }

    fn updates_of(stack_id: &str) -> Vec<Value> {
        match stack_id {
            "stk1" => (0..UPDATE_COUNT).map(listed_update).collect(),
            "dup" => vec![
                json!({"id": "d1", "start_ts": 7, "status": "Complete", "success": true}),
                json!({"id": "d2", "start_ts": 7, "status": "Complete", "success": true}),
            ],
            "id-dead" => (0..BUSY_UPDATES)
                .map(|i| stack_update(stack_id, i, BUSY_START - i as i64 * 1_000))
                .collect(),
            "id-running" => (0..RUNNING_UPDATES)
                .map(|i| stack_update(stack_id, i, RUNNING_START - i as i64 * 1_000))
                .collect(),
            "id-down" => vec![stack_update(stack_id, 0, DOWN_START)],
            _ => Vec::new(),
        }
    }

    fn queried_stack_ids(query: &Value) -> Vec<String> {
        let target = &query["target.id"];
        match target.as_str() {
            Some(id) => vec![id.to_string()],
            None => target["$in"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect(),
        }
    }

    fn list_updates(query: &Value, page: usize) -> (u16, Value) {
        if query["target.type"] != "Stack" {
            return (
                400,
                json!({"error": "expected a Stack target", "trace": []}),
            );
        }
        let mut all: Vec<Value> = queried_stack_ids(query)
            .iter()
            .flat_map(|id| updates_of(id))
            .collect();
        all.sort_by_key(|update| std::cmp::Reverse(update["start_ts"].as_i64()));
        let updates: Vec<Value> = all
            .into_iter()
            .filter(|update| {
                query
                    .get("start_ts")
                    .is_none_or(|start_ts| *start_ts == update["start_ts"])
            })
            .skip(page * UPDATES_PER_PAGE)
            .take(UPDATES_PER_PAGE)
            .collect();
        let next_page = (updates.len() == UPDATES_PER_PAGE).then_some(page + 1);
        (200, json!({"updates": updates, "next_page": next_page}))
    }

    fn reply(request: &Recorded) -> (u16, Value) {
        let header = |name: &str| request.headers.get(name).map(String::as_str);
        if header("x-api-key") == Some("K-denied") && header("x-api-secret") == Some(API_SECRET) {
            return (403, json!({"error": "permission denied", "trace": []}));
        }
        if header("x-api-key") != Some(API_KEY) || header("x-api-secret") != Some(API_SECRET) {
            return (
                401,
                json!({"error": "invalid api key or secret", "trace": []}),
            );
        }
        let params = &request.body["params"];
        match (
            request.path.as_str(),
            request.body["type"].as_str().unwrap_or_default(),
        ) {
            ("/read", "GetVersion") => (200, json!({"version": "2.3.4"})),
            ("/read", "ListStacks") => (200, stacks()),
            ("/read", "ListUpdates") => list_updates(
                &params["query"],
                params["page"].as_u64().unwrap_or_default() as usize,
            ),
            ("/read", "GetUpdate") => {
                let index = params["id"]
                    .as_str()
                    .and_then(|id| id.strip_prefix('u'))
                    .and_then(|index| index.parse().ok());
                match index {
                    Some(index) if index < UPDATE_COUNT => (200, full_update(index)),
                    _ => (404, json!({"error": "update not found", "trace": []})),
                }
            }
            ("/execute", "DeployStack") if params["stack"] == "boom" => {
                (503, json!({"error": "core is restarting", "trace": []}))
            }
            ("/execute", "DeployStack") => (
                200,
                json!({
                    "_id": {"$oid": "u-new"}, "operation": "DeployStack",
                    "start_ts": NEWEST_START + 5_000, "success": false, "operator": "usr1",
                    "target": {"type": "Stack", "id": params["stack"]}, "logs": [],
                    "end_ts": null, "status": "InProgress"
                }),
            ),
            (path, kind) => (
                404,
                json!({"error": format!("unexpected {path} {kind}"), "trace": []}),
            ),
        }
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Recorded> {
        let mut raw = Vec::new();
        let mut buffer = [0; 4096];
        let (head_end, content_length) = loop {
            let count = stream.read(&mut buffer).await.ok()?;
            if count == 0 {
                return None;
            }
            raw.extend_from_slice(&buffer[..count]);
            if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&raw[..position]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0);
                break (position + 4, length);
            }
        };
        while raw.len() < head_end + content_length {
            let count = stream.read(&mut buffer).await.ok()?;
            if count == 0 {
                return None;
            }
            raw.extend_from_slice(&buffer[..count]);
        }

        let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
        let mut lines = head.lines();
        let path = lines.next()?.split_whitespace().nth(1)?.to_string();
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_string()))
            .collect();
        let body = serde_json::from_slice(&raw[head_end..head_end + content_length])
            .unwrap_or(Value::Null);
        Some(Recorded {
            path,
            headers,
            body,
        })
    }

    async fn stub_komodo() -> (String, Requests) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Requests::default();
        let recorded = requests.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let recorded = recorded.clone();
                tokio::spawn(async move {
                    let Some(request) = read_request(&mut stream).await else {
                        return;
                    };
                    let (status, body) = reply(&request);
                    recorded.lock().await.push(request);
                    let body = body.to_string();
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (format!("http://{addr}"), requests)
    }

    fn plugin_with(
        server_url: &str, api_key: &str, secret: &str, extra: &[(&str, &str)],
    ) -> KomodoPlugin {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut pairs = vec![
            ("server_url", server_url),
            ("api_key", api_key),
            ("token", secret),
        ];
        pairs.extend_from_slice(extra);
        let mut plugin = KomodoPlugin::new();
        plugin
            .initialize(1, cfg(&pairs), Some(Arc::new(reqwest::Client::new())))
            .unwrap();
        plugin
    }

    async fn stub_plugin(extra: &[(&str, &str)]) -> (KomodoPlugin, String, Requests) {
        let (server_url, requests) = stub_komodo().await;
        let plugin = plugin_with(&format!("{server_url}/"), API_KEY, API_SECRET, extra);
        (plugin, server_url, requests)
    }

    async fn bodies(requests: &Requests, request_type: &str) -> Vec<Value> {
        requests
            .lock()
            .await
            .iter()
            .filter(|request| request.body["type"] == request_type)
            .map(|request| request.body.clone())
            .collect()
    }

    async fn list_update_pages(requests: &Requests) -> Vec<u64> {
        bodies(requests, "ListUpdates")
            .await
            .iter()
            .map(|body| body["params"]["page"].as_u64().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn stacks_become_pipelines_with_every_state_mapped() {
        let (plugin, _, requests) = stub_plugin(&[]).await;

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        let summary: Vec<_> = pipelines
            .iter()
            .map(|pipeline| (pipeline.name.as_str(), pipeline.status))
            .collect();
        assert_eq!(
            summary,
            [
                ("stack-deploying", PipelineStatus::Running),
                ("stack-running", PipelineStatus::Success),
                ("stack-paused", PipelineStatus::Cancelled),
                ("stack-stopped", PipelineStatus::Cancelled),
                ("stack-created", PipelineStatus::Pending),
                ("stack-restarting", PipelineStatus::Failed),
                ("stack-dead", PipelineStatus::Failed),
                ("stack-removing", PipelineStatus::Running),
                ("stack-unhealthy", PipelineStatus::Failed),
                ("stack-down", PipelineStatus::Cancelled),
                ("stack-unknown", PipelineStatus::Pending),
            ]
        );
        assert_eq!(pipelines[0].id, "komodo__1__id-deploying");
        assert_eq!(pipelines[0].repository, "prod-1/stack-deploying");
        assert_eq!(pipelines[0].branch.as_deref(), Some("main"));
        assert_eq!(pipelines[0].metadata["server"], json!("prod-1"));
        assert_eq!(pipelines[0].metadata["update_available"], json!(false));
        assert_eq!(pipelines[1].metadata["update_available"], json!(true));

        let requests = requests.lock().await;
        assert_eq!(requests[0].path, "/read");
        assert_eq!(
            requests[0].body,
            json!({"type": "ListStacks", "params": {"query": {}, "limit": 0}})
        );
        assert_eq!(requests[1].body["type"], "ListUpdates");
        assert_eq!(
            requests[1].body["params"]["query"]["target.id"]["$in"]
                .as_array()
                .map(Vec::len),
            Some(STATES.len())
        );
    }

    #[tokio::test]
    async fn selection_keeps_stacks_matching_an_id_or_a_name() {
        let (plugin, _, _) = stub_plugin(&[("selected_items", "stack-running, id-down")]).await;
        let names: Vec<_> = plugin
            .fetch_pipelines()
            .await
            .unwrap()
            .into_iter()
            .map(|pipeline| pipeline.name)
            .collect();
        assert_eq!(names, ["stack-running", "stack-down"]);

        let (nothing, _, _) = stub_plugin(&[("selected_items", "")]).await;
        assert!(nothing.fetch_pipelines().await.unwrap().is_empty());
    }

    async fn last_runs(extra: &[(&str, &str)]) -> (HashMap<String, Option<i64>>, Vec<u64>, Value) {
        let (plugin, _, requests) = stub_plugin(extra).await;
        let pipelines = plugin.fetch_pipelines().await.unwrap();
        let last_runs = pipelines
            .into_iter()
            .map(|pipeline| {
                (
                    pipeline.name,
                    pipeline.last_run.map(|at| at.timestamp_millis()),
                )
            })
            .collect();
        let first_lookup = bodies(&requests, "ListUpdates").await.remove(0);
        (
            last_runs,
            list_update_pages(&requests).await,
            first_lookup["params"]["query"].clone(),
        )
    }

    #[tokio::test]
    async fn last_run_is_the_newest_update_start_and_none_without_updates() {
        let (last_runs, pages, query) =
            last_runs(&[("selected_items", "id-running,id-down,id-paused")]).await;

        assert_eq!(last_runs["stack-running"], Some(RUNNING_START));
        assert_eq!(last_runs["stack-down"], Some(DOWN_START));
        assert_eq!(last_runs["stack-paused"], None);
        assert_eq!(pages, [0, 1]);
        assert_eq!(
            query,
            json!({
                "target.type": "Stack",
                "target.id": {"$in": ["id-running", "id-paused", "id-down"]}
            })
        );
    }

    #[tokio::test]
    async fn update_paging_stops_once_every_stack_is_seen() {
        let (last_runs, pages, _) = last_runs(&[("selected_items", "id-running")]).await;

        assert_eq!(last_runs["stack-running"], Some(RUNNING_START));
        assert_eq!(pages, [0]);
    }

    #[tokio::test]
    async fn update_paging_is_capped_when_a_stack_stays_unseen() {
        let (last_runs, pages, _) = last_runs(&[("selected_items", "id-dead,id-down")]).await;

        assert_eq!(last_runs["stack-dead"], Some(BUSY_START));
        assert_eq!(last_runs["stack-down"], None);
        assert_eq!(pages, [0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn no_update_lookup_is_sent_without_stacks() {
        let (plugin, _, requests) = stub_plugin(&[("selected_items", "")]).await;

        assert!(plugin.fetch_pipelines().await.unwrap().is_empty());

        assert!(bodies(&requests, "ListUpdates").await.is_empty());
    }

    #[tokio::test]
    async fn picker_lists_stacks_by_id_with_paging_search_and_server_filter() {
        let (plugin, _, _) = stub_plugin(&[]).await;
        let page = |page, page_size| Some(PaginationParams { page, page_size });

        let second = plugin.fetch_available_pipelines(page(2, 5)).await.unwrap();
        let third = plugin.fetch_available_pipelines(page(3, 5)).await.unwrap();
        let found = plugin
            .fetch_available_pipelines_filtered(
                Some("prod-1".to_string()),
                Some("DEAD".to_string()),
                None,
            )
            .await
            .unwrap();
        let elsewhere = plugin
            .fetch_available_pipelines_filtered(Some("prod-2".to_string()), None, None)
            .await
            .unwrap();
        let organizations = plugin.fetch_organizations().await.unwrap();

        let ids = |response: &PaginatedResponse<AvailablePipeline>| {
            response
                .items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&second),
            [
                "id-restarting",
                "id-dead",
                "id-removing",
                "id-unhealthy",
                "id-down"
            ]
        );
        assert_eq!(
            (second.total_count, second.total_pages, second.has_more),
            (11, 3, true)
        );
        assert_eq!(ids(&third), ["id-unknown"]);
        assert!(!third.has_more);
        assert_eq!(ids(&found), ["id-dead"]);
        assert!(elsewhere.items.is_empty());
        assert_eq!(
            organizations
                .iter()
                .map(|o| o.name.as_str())
                .collect::<Vec<_>>(),
            ["prod-1"]
        );
    }

    #[tokio::test]
    async fn run_history_follows_next_page_until_the_limit_is_met() {
        for (limit, expected_runs, expected_pages) in [
            (0, 0, vec![]),
            (1, 1, vec![0]),
            (100, 100, vec![0]),
            (101, 101, vec![0, 1]),
            (1000, UPDATE_COUNT, vec![0, 1, 2]),
        ] {
            let (plugin, _, requests) = stub_plugin(&[]).await;

            let runs = plugin.fetch_run_history(PIPELINE, limit).await.unwrap();

            assert_eq!(runs.len(), expected_runs, "limit {limit}");
            assert_eq!(
                list_update_pages(&requests).await,
                expected_pages,
                "limit {limit}"
            );
            assert_eq!(
                runs.iter()
                    .map(|run| run.run_number)
                    .collect::<HashSet<_>>()
                    .len(),
                runs.len()
            );
        }
    }

    #[tokio::test]
    async fn run_history_maps_updates_of_the_stack_newest_first() {
        let (plugin, server_url, requests) = stub_plugin(&[]).await;

        let runs = plugin.fetch_run_history(PIPELINE, 150).await.unwrap();

        assert!(runs
            .windows(2)
            .all(|pair| pair[0].run_number > pair[1].run_number));
        assert_eq!(runs[0].run_number, NEWEST_START);
        assert_eq!(runs[0].status, PipelineStatus::Running);
        assert_eq!(runs[1].status, PipelineStatus::Success);
        assert_eq!(runs[5].status, PipelineStatus::Failed);
        assert_eq!(runs[1].actor.as_deref(), Some("ada"));
        assert_eq!(runs[1].metadata["operation"], json!("StartStack"));
        assert_eq!(runs[1].pipeline_id, PIPELINE);
        assert_eq!(runs[1].logs_url, format!("{server_url}/updates/u001"));
        assert_eq!(runs[1].duration_seconds, None);
        assert_eq!(
            bodies(&requests, "ListUpdates").await[0]["params"],
            json!({"query": {"target.type": "Stack", "target.id": "stk1"}, "page": 0})
        );
    }

    #[tokio::test]
    async fn run_details_resolve_a_run_number_to_one_update() {
        let (plugin, server_url, requests) = stub_plugin(&[]).await;
        let runs = plugin.fetch_run_history(PIPELINE, 10).await.unwrap();
        let listed = &runs[4];

        let details = plugin
            .fetch_run_details(PIPELINE, listed.run_number)
            .await
            .unwrap();

        assert_eq!(details.id, listed.id);
        assert_eq!(details.run_number, listed.run_number);
        assert_eq!(details.status, PipelineStatus::Success);
        assert_eq!(details.actor.as_deref(), Some("ada"));
        assert_eq!(details.duration_seconds, Some(42));
        assert_eq!(
            details.concluded_at.map(|at| at.timestamp_millis()),
            Some(listed.run_number + 42_500)
        );
        assert_eq!(details.commit_sha.as_deref(), Some("abc1234"));
        assert_eq!(details.logs_url, format!("{server_url}/updates/u004"));
        let lookups = bodies(&requests, "ListUpdates").await;
        assert_eq!(
            lookups.last().unwrap()["params"]["query"],
            json!({"target.type": "Stack", "target.id": "stk1", "start_ts": listed.run_number})
        );
        assert_eq!(
            bodies(&requests, "GetUpdate").await,
            [json!({"type": "GetUpdate", "params": {"id": "u004"}})]
        );

        let running = plugin
            .fetch_run_details(PIPELINE, NEWEST_START)
            .await
            .unwrap();
        assert_eq!(running.status, PipelineStatus::Running);
        assert_eq!(running.concluded_at, None);
        assert_eq!(running.duration_seconds, None);
    }

    #[tokio::test]
    async fn run_details_for_an_unknown_run_number_are_not_found() {
        let (plugin, _, requests) = stub_plugin(&[]).await;

        let error = plugin
            .fetch_run_details(PIPELINE, 12_345)
            .await
            .unwrap_err();

        assert!(matches!(error, PluginError::PipelineNotFound(_)), "{error}");
        assert!(bodies(&requests, "GetUpdate").await.is_empty());
    }

    #[tokio::test]
    async fn run_details_refuse_when_two_updates_share_a_run_number() {
        let (plugin, _, _) = stub_plugin(&[]).await;

        let error = plugin
            .fetch_run_details("komodo__1__dup", 7)
            .await
            .unwrap_err();

        assert!(
            matches!(&error, PluginError::ApiError(m) if m.contains("share run number")),
            "{error}"
        );
    }

    #[tokio::test]
    async fn trigger_deploys_the_stack_and_returns_the_new_update() {
        let (plugin, server_url, requests) = stub_plugin(&[]).await;

        let result = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: PIPELINE.to_string(),
                inputs: None,
            })
            .await
            .unwrap();

        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result["run_number"], json!(NEWEST_START + 5_000));
        assert_eq!(
            result["logs_url"],
            json!(format!("{server_url}/updates/u-new"))
        );
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/execute");
        assert_eq!(
            requests[0].body,
            json!({"type": "DeployStack", "params": {"stack": "stk1"}})
        );
        assert_eq!(requests[0].headers["x-api-key"], API_KEY);
        assert_eq!(requests[0].headers["x-api-secret"], API_SECRET);
        assert_eq!(requests[0].headers["content-type"], "application/json");
    }

    #[tokio::test]
    async fn deploy_is_not_resent_after_a_server_error() {
        let (plugin, _, requests) = stub_plugin(&[]).await;

        let error = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: "komodo__1__boom".to_string(),
                inputs: None,
            })
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error}"
        );
        assert_eq!(bodies(&requests, "DeployStack").await.len(), 1);
    }

    #[tokio::test]
    async fn validation_calls_get_version_and_rejects_bad_credentials() {
        let (plugin, server_url, requests) = stub_plugin(&[]).await;
        let wrong_secret = plugin_with(&server_url, API_KEY, "S-wrong", &[]);
        let denied = plugin_with(&server_url, "K-denied", API_SECRET, &[]);

        assert!(plugin.validate_credentials().await.unwrap());
        let unauthorized = wrong_secret.validate_credentials().await.unwrap_err();
        let forbidden = denied.validate_credentials().await.unwrap_err();

        assert_eq!(
            bodies(&requests, "GetVersion").await[0],
            json!({"type": "GetVersion", "params": {}})
        );
        assert!(
            matches!(&unauthorized, PluginError::AuthenticationFailed(m) if m.contains("invalid api key")),
            "{unauthorized}"
        );
        assert!(
            matches!(forbidden, PluginError::PermissionDenied(_)),
            "{forbidden}"
        );
    }

    #[tokio::test]
    async fn cancel_is_not_supported_for_stacks() {
        let (plugin, _, requests) = stub_plugin(&[]).await;

        let error = plugin.cancel_run(PIPELINE, NEWEST_START).await.unwrap_err();

        assert!(matches!(error, PluginError::NotSupported(_)), "{error}");
        assert!(!plugin.metadata().capabilities.cancel);
        assert!(requests.lock().await.is_empty());
    }

    #[tokio::test]
    async fn foreign_pipeline_ids_are_rejected_before_any_request() {
        let (plugin, _, requests) = stub_plugin(&[]).await;

        let history = plugin.fetch_run_history("argocd__1__web", 10).await;
        let details = plugin.fetch_run_details("komodo__x__stk1", 1).await;
        let trigger = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: "komodo__1".to_string(),
                inputs: None,
            })
            .await;

        for result in [history.map(drop), details.map(drop), trigger.map(drop)] {
            assert!(matches!(result, Err(PluginError::InvalidConfig(_))));
        }
        assert!(requests.lock().await.is_empty());
    }

    #[test]
    fn initialize_requires_url_credentials_and_a_client() {
        let complete = [
            ("server_url", "https://komodo.example.com"),
            ("api_key", "K"),
            ("token", "S"),
        ];
        let client = || Some(Arc::new(reqwest::Client::new()));
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut plugin = KomodoPlugin::new();

        assert!(matches!(
            plugin.initialize(1, cfg(&complete), None),
            Err(PluginError::Internal(_))
        ));
        for missing in ["server_url", "api_key", "token"] {
            let pairs: Vec<_> = complete
                .iter()
                .copied()
                .filter(|(key, _)| *key != missing)
                .collect();
            assert!(
                matches!(
                    plugin.initialize(1, cfg(&pairs), client()),
                    Err(PluginError::InvalidConfig(_))
                ),
                "{missing}"
            );
        }
        assert!(plugin.initialize(1, cfg(&complete), client()).is_ok());
    }

    #[tokio::test]
    async fn an_uninitialized_plugin_reports_an_internal_error() {
        let plugin = KomodoPlugin::new();

        assert!(matches!(
            plugin.fetch_pipelines().await,
            Err(PluginError::Internal(_))
        ));
        assert!(matches!(
            plugin.validate_credentials().await,
            Err(PluginError::Internal(_))
        ));
    }

    #[test]
    fn client_options_carry_the_origin_and_the_insecure_flag() {
        let plugin = KomodoPlugin::new();

        let options = plugin.client_options(&cfg(&[
            ("server_url", " https://Komodo.example.com:9120/ "),
            ("insecure", "true"),
        ]));
        let defaults = plugin.client_options(&cfg(&[("server_url", "not a url")]));

        assert_eq!(
            options.origin.as_ref().map(|url| url.as_str()),
            Some("https://komodo.example.com:9120/")
        );
        assert!(options.accept_invalid_certs);
        assert_eq!(defaults.origin, None);
        assert!(!defaults.accept_invalid_certs);
    }
}
