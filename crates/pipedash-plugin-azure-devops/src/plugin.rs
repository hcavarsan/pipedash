use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::try_join_all;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

pub struct AzureDevOpsPlugin {
    metadata: PluginMetadata,
    client: Option<client::AzureDevOpsClient>,
    provider_id: Option<i64>,
    organization: String,
    config: HashMap<String, String>,
    status_source: Option<StatusSource>,
}

impl Default for AzureDevOpsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AzureDevOpsPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            organization: String::new(),
            config: HashMap::new(),
            status_source: None,
        }
    }

    fn client(&self) -> PluginResult<&client::AzureDevOpsClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn provider_id(&self) -> PluginResult<i64> {
        self.provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))
    }

    async fn fetch_project_pipelines(
        &self, provider_id: i64, project: &str,
    ) -> PluginResult<Vec<Pipeline>> {
        let definitions = self.client()?.list_definitions(project).await?;
        Ok(definitions
            .iter()
            .map(|definition| {
                mapper::map_pipeline(provider_id, &self.organization, project, definition)
            })
            .collect())
    }

    async fn owned_build(
        &self, project: &str, definition_id: i64, pipeline_id: &str, build_id: i64,
    ) -> PluginResult<types::Build> {
        let build = self.client()?.get_build(project, build_id).await?;
        let belongs_to_pipeline = build
            .definition
            .as_ref()
            .and_then(|definition| definition.id)
            .is_none_or(|id| id == definition_id);
        if !belongs_to_pipeline {
            return Err(PluginError::PipelineNotFound(format!(
                "Run #{build_id} does not belong to {pipeline_id}"
            )));
        }
        Ok(build)
    }
}

#[async_trait]
impl Plugin for AzureDevOpsPlugin {
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
        let server_url = config::get_server_url(&config)?;
        let token = config
            .get("token")
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                PluginError::InvalidConfig("Missing Azure DevOps personal access token".to_string())
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

        self.organization = config::organization_of(&server_url);
        self.status_source = status_source;
        self.client = Some(client::AzureDevOpsClient::new(
            http_client,
            server_url,
            token,
        ));
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
        self.client()?;
        Ok(vec![Organization {
            id: self.organization.clone(),
            name: self.organization.clone(),
            description: None,
        }])
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        Ok(retain_with_ci(ids, |project| async move {
            client.has_definitions(&project).await
        })
        .await)
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedAvailablePipelines> {
        let client = self.client()?;
        let params = params.unwrap_or_default();
        let start = params
            .calculate_offset()
            .map_err(PluginError::InvalidConfig)?;

        let mut projects = if org.is_none_or(|org| org == self.organization) {
            client.list_projects().await?
        } else {
            Vec::new()
        };
        if let Some(term) = search.as_deref().map(str::to_lowercase) {
            projects.retain(|project| {
                project.name.to_lowercase().contains(&term)
                    || project
                        .description
                        .as_deref()
                        .is_some_and(|description| description.to_lowercase().contains(&term))
            });
        }
        projects.sort_by_cached_key(|project| project.name.to_lowercase());

        let total_count = projects.len();
        let items = projects
            .iter()
            .skip(start)
            .take(params.page_size)
            .map(|project| mapper::map_available_pipeline(project, &self.organization))
            .collect::<Vec<_>>();
        let mut response =
            PaginatedResponse::new(items, params.page, params.page_size, total_count);
        response.has_more = start + response.items.len() < total_count;
        Ok(response)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self.provider_id()?;
        let projects = config::get_projects(&self.config);
        if projects.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No projects configured".to_string(),
            ));
        }

        let results = try_join_all(
            projects
                .iter()
                .map(|project| self.fetch_project_pipelines(provider_id, project)),
        )
        .await?;

        Ok(results.into_iter().flatten().collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let builds = client
            .list_builds(id.project, id.definition_id, limit)
            .await?;
        Ok(builds
            .iter()
            .map(|build| mapper::map_run(build, pipeline_id, id.project, client.server_url()))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let build = self
            .owned_build(id.project, id.definition_id, pipeline_id, run_number)
            .await?;
        Ok(mapper::map_run(
            &build,
            pipeline_id,
            id.project,
            self.client()?.server_url(),
        ))
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let id = config::parse_pipeline_id(workflow_id)?;
        let definition = self
            .client()?
            .get_definition(id.project, id.definition_id)
            .await?;

        let default_branch = definition
            .repository
            .and_then(|repository| repository.default_branch)
            .filter(|branch| !branch.is_empty())
            .map(|branch| mapper::short_branch(&branch).to_string());
        let mut parameters = vec![WorkflowParameter {
            name: mapper::BRANCH_INPUT.to_string(),
            label: Some("Branch".to_string()),
            description: Some(
                "Branch to build. The definition's default branch is used when empty".to_string(),
            ),
            param_type: WorkflowParameterType::String {
                default: default_branch,
            },
            required: false,
        }];
        parameters.extend(
            definition
                .variables
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, variable)| variable.allow_override && !variable.is_secret)
                .map(|(name, variable)| WorkflowParameter {
                    label: Some(name.clone()),
                    description: Some("Pipeline variable".to_string()),
                    param_type: WorkflowParameterType::String {
                        default: variable.value,
                    },
                    required: false,
                    name,
                }),
        );
        Ok(parameters)
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let id = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;
        let body = mapper::trigger_body(id.definition_id, params.inputs);
        let build = client.queue_build(id.project, &body).await?;
        let logs_url = build
            .web_link()
            .map(str::to_string)
            .unwrap_or_else(|| config::build_web_url(client.server_url(), id.project, build.id));
        let branch = body["sourceBranch"]
            .as_str()
            .map(mapper::short_branch)
            .unwrap_or("the default branch");
        Ok(serde_json::json!({
            "message": format!("Queued build {} on {branch}", build.id),
            "run_number": build.id,
            "logs_url": logs_url
        })
        .to_string())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        self.owned_build(id.project, id.definition_id, pipeline_id, run_number)
            .await?;
        client.cancel_build(id.project, run_number).await
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

    const BASIC_AUTH: &str = "Basic OnN0dWItdG9rZW4=";

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
        content_type: &'static str,
        continuation: Option<String>,
        body: String,
    }

    impl Reply {
        fn json(status: u16, body: Value) -> Self {
            Self {
                status,
                content_type: "application/json; charset=utf-8",
                continuation: None,
                body: body.to_string(),
            }
        }

        fn html(status: u16, body: &str) -> Self {
            Self {
                status,
                content_type: "text/html; charset=utf-8",
                continuation: None,
                body: body.to_string(),
            }
        }

        fn not_found(request: &Request) -> Self {
            Self::json(
                404,
                json!({"message": format!("unexpected {} {}", request.method, request.url)}),
            )
        }

        fn continued(mut self, token: &str) -> Self {
            self.continuation = Some(token.to_string());
            self
        }

        fn to_http(&self) -> String {
            let continuation = self
                .continuation
                .as_ref()
                .map(|token| format!("x-ms-continuationtoken: {token}\r\n"))
                .unwrap_or_default();
            format!(
                "HTTP/1.1 {} X\r\nContent-Type: {}\r\n{continuation}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                self.status,
                self.content_type,
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

        fn requests_to(&self, method: &str, path: &str) -> Vec<(Option<String>, Option<String>)> {
            self.requests()
                .iter()
                .filter(|request| request.method == method && request.url.path() == path)
                .map(|request| (request.query("$top"), request.query("continuationToken")))
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

    fn plugin_for(stub: &Stub, organization_suffix: &str, selected: &str) -> AzureDevOpsPlugin {
        let mut plugin = AzureDevOpsPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    (
                        "server_url".to_string(),
                        format!("{}/acme{organization_suffix}", stub.base_url),
                    ),
                    ("token".to_string(), "stub-token".to_string()),
                    ("selected_items".to_string(), selected.to_string()),
                ]),
                None,
            )
            .unwrap();
        plugin
    }

    fn build_json(id: i64, status: &str, result: Option<&str>, definition_id: i64) -> Value {
        let mut build = json!({
            "id": id,
            "buildNumber": format!("20261028.{}", id % 100),
            "status": status,
            "sourceBranch": "refs/heads/main",
            "sourceVersion": format!("sha{id}"),
            "queueTime": "2026-10-28T10:00:00.1234567Z",
            "startTime": "2026-10-28T10:00:05.5Z",
            "requestedFor": {"displayName": "Ada Lovelace"},
            "definition": {"id": definition_id},
            "_links": {"web": {"href": format!("https://dev.azure.com/acme/Alpha/_build/results?buildId={id}")}}
        });
        if let Some(result) = result {
            build["result"] = json!(result);
            build["finishTime"] = json!("2026-10-28T10:02:35.5Z");
        }
        build
    }

    fn azure_reply(request: &Request) -> Reply {
        let token = request.query("continuationToken");
        match (request.method.as_str(), request.url.path()) {
            ("GET", "/acme/_apis/projects") => match token.as_deref() {
                None => Reply::json(
                    200,
                    json!({"count": 2, "value": [
                        {"id": "p1", "name": "Alpha", "description": "Main product"},
                        {"id": "p2", "name": "Beta Team"}
                    ]}),
                )
                .continued("projects-2"),
                Some("projects-2") => {
                    Reply::json(200, json!({"count": 1, "value": [{"id": "p3", "name": "Empty"}]}))
                }
                Some(_) => Reply::not_found(request),
            },
            ("GET", "/acme/Alpha/_apis/build/definitions") => match token.as_deref() {
                None => Reply::json(
                    200,
                    json!({"count": 2, "value": [
                        {"id": 1, "name": "CI", "latestBuild": build_json(903, "completed", Some("succeeded"), 1)},
                        {"id": 2, "name": "Docs"}
                    ]}),
                )
                .continued("defs-2"),
                Some("defs-2") => Reply::json(
                    200,
                    json!({"count": 2, "value": [
                        {"id": 3, "name": "Deploy", "latestBuild": build_json(905, "inProgress", None, 3)},
                        {"id": 4, "name": "Nightly", "latestBuild": build_json(906, "completed", Some("partiallySucceeded"), 4)}
                    ]}),
                ),
                Some(_) => Reply::not_found(request),
            },
            ("GET", "/acme/Beta%20Team/_apis/build/definitions") => Reply::json(
                200,
                json!({"count": 1, "value": [
                    {"id": 5, "name": "Mobile", "latestBuild": build_json(907, "completed", Some("canceled"), 5)}
                ]}),
            ),
            ("GET", "/acme/Empty/_apis/build/definitions") => {
                Reply::json(200, json!({"count": 0, "value": []}))
            }
            ("GET", "/acme/Alpha/_apis/build/definitions/1") => Reply::json(
                200,
                json!({
                    "id": 1, "name": "CI",
                    "repository": {"defaultBranch": "refs/heads/main"},
                    "variables": {
                        "deployTarget": {"value": "dev", "allowOverride": true},
                        "fixed": {"value": "x"},
                        "apiKey": {"isSecret": true, "allowOverride": true},
                        "branch": {"value": "legacy", "allowOverride": true}
                    }
                }),
            ),
            ("GET", "/acme/Alpha/_apis/build/builds") => {
                let history = [
                    build_json(923, "inProgress", None, 1),
                    build_json(922, "completed", Some("failed"), 1),
                    build_json(921, "completed", Some("succeeded"), 1),
                ];
                let start = match token.as_deref() {
                    None => 0,
                    Some("builds-2") => 2,
                    Some(_) => return Reply::not_found(request),
                };
                let size = request
                    .query("$top")
                    .and_then(|top| top.parse().ok())
                    .unwrap_or(history.len())
                    .min(2);
                let page: Vec<_> = history[start..].iter().take(size).collect();
                let reply = Reply::json(200, json!({"count": page.len(), "value": page}));
                if start + size < history.len() {
                    reply.continued("builds-2")
                } else {
                    reply
                }
            }
            ("GET", "/acme/Alpha/_apis/build/builds/923") => {
                Reply::json(200, build_json(923, "inProgress", None, 1))
            }
            ("GET", "/acme/Alpha/_apis/build/builds/922") => {
                Reply::json(200, build_json(922, "completed", Some("failed"), 1))
            }
            ("GET", "/acme/Alpha/_apis/build/builds/777") => {
                Reply::json(200, build_json(777, "completed", Some("succeeded"), 9))
            }
            ("POST", "/acme/Alpha/_apis/build/builds") => {
                Reply::json(200, build_json(930, "notStarted", None, 1))
            }
            ("PATCH", "/acme/Alpha/_apis/build/builds/923") => {
                Reply::json(200, build_json(923, "cancelling", None, 1))
            }
            _ => Reply::not_found(request),
        }
    }

    #[tokio::test]
    async fn projects_are_listed_across_continuation_pages_and_paged_for_the_picker() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "");

        let first = plugin
            .fetch_available_pipelines(Some(PaginationParams {
                page: 1,
                page_size: 2,
            }))
            .await
            .unwrap();
        let second = plugin
            .fetch_available_pipelines(Some(PaginationParams {
                page: 2,
                page_size: 2,
            }))
            .await
            .unwrap();

        let names = |page: &PaginatedAvailablePipelines| {
            page.items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&first), ["Alpha", "Beta Team"]);
        assert!(first.has_more);
        assert_eq!(first.total_count, 3);
        assert_eq!(first.items[0].description.as_deref(), Some("Main product"));
        assert_eq!(first.items[0].organization.as_deref(), Some("acme"));
        assert_eq!(names(&second), ["Empty"]);
        assert!(!second.has_more);

        assert_eq!(
            stub.requests_to("GET", "/acme/_apis/projects"),
            [
                (Some("100".to_string()), None),
                (Some("100".to_string()), Some("projects-2".to_string())),
                (Some("100".to_string()), None),
                (Some("100".to_string()), Some("projects-2".to_string())),
            ]
        );
        assert!(stub.requests().iter().all(|request| {
            request.query("api-version").as_deref() == Some("7.0")
                && request.authorization.as_deref() == Some(BASIC_AUTH)
        }));
    }

    #[tokio::test]
    async fn picker_filters_by_organization_and_search_term() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "");

        let found = plugin
            .fetch_available_pipelines_filtered(
                Some("acme".to_string()),
                Some("TEAM".to_string()),
                None,
            )
            .await
            .unwrap();
        assert_eq!(found.items.len(), 1);
        assert_eq!(found.items[0].name, "Beta Team");

        let requests_before = stub.requests().len();
        let other = plugin
            .fetch_available_pipelines_filtered(Some("other".to_string()), None, None)
            .await
            .unwrap();
        assert!(other.items.is_empty());
        assert_eq!(stub.requests().len(), requests_before);

        let organizations = plugin.fetch_organizations().await.unwrap();
        assert_eq!(organizations.len(), 1);
        assert_eq!(organizations[0].id, "acme");
        assert_eq!(organizations[0].name, "acme");
    }

    #[tokio::test]
    async fn definitions_of_selected_projects_become_pipelines_across_continuation_pages() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha, Beta Team");

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        let summary: Vec<_> = pipelines
            .iter()
            .map(|pipeline| {
                (
                    pipeline.id.as_str(),
                    pipeline.name.as_str(),
                    pipeline.status,
                    pipeline.repository.as_str(),
                    pipeline.branch.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "azure-devops__1__Alpha/1",
                    "CI",
                    PipelineStatus::Success,
                    "acme/Alpha",
                    Some("main")
                ),
                (
                    "azure-devops__1__Alpha/2",
                    "Docs",
                    PipelineStatus::Pending,
                    "acme/Alpha",
                    None
                ),
                (
                    "azure-devops__1__Alpha/3",
                    "Deploy",
                    PipelineStatus::Running,
                    "acme/Alpha",
                    Some("main")
                ),
                (
                    "azure-devops__1__Alpha/4",
                    "Nightly",
                    PipelineStatus::Failed,
                    "acme/Alpha",
                    Some("main")
                ),
                (
                    "azure-devops__1__Beta Team/5",
                    "Mobile",
                    PipelineStatus::Cancelled,
                    "acme/Beta Team",
                    Some("main")
                ),
            ]
        );
        assert!(pipelines
            .iter()
            .all(|pipeline| pipeline.provider_type == "azure-devops" && pipeline.provider_id == 1));
        assert_eq!(
            stub.requests_to("GET", "/acme/Alpha/_apis/build/definitions"),
            [
                (Some("100".to_string()), None),
                (Some("100".to_string()), Some("defs-2".to_string())),
            ]
        );
        assert!(stub
            .requests()
            .iter()
            .filter(|request| request.url.path().ends_with("/build/definitions"))
            .all(|request| request.query("includeLatestBuilds").as_deref() == Some("true")));
    }

    #[tokio::test]
    async fn fetching_pipelines_without_selected_projects_is_a_configuration_error() {
        let stub = serve(azure_reply).await;
        let error = plugin_for(&stub, "", "")
            .fetch_pipelines()
            .await
            .unwrap_err();
        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error}");
    }

    #[tokio::test]
    async fn a_trailing_slash_on_the_server_url_does_not_double_the_slash() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "/", "Alpha");

        plugin.fetch_available_pipelines(None).await.unwrap();
        plugin.fetch_pipelines().await.unwrap();
        plugin.validate_credentials().await.unwrap();

        let paths: Vec<_> = stub
            .requests()
            .iter()
            .map(|request| request.url.path().to_string())
            .collect();
        assert!(!paths.is_empty());
        assert!(paths.iter().all(|path| !path.contains("//")), "{paths:?}");
        assert!(
            paths.iter().all(|path| path.starts_with("/acme/")),
            "{paths:?}"
        );
    }

    #[tokio::test]
    async fn run_history_is_newest_first_and_follows_continuation_until_the_limit() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        let runs = plugin
            .fetch_run_history("azure-devops__1__Alpha/1", 3)
            .await
            .unwrap();

        let summary: Vec<_> = runs
            .iter()
            .map(|run| (run.run_number, run.status, run.id.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (923, PipelineStatus::Running, "azure-devops-run-923"),
                (922, PipelineStatus::Failed, "azure-devops-run-922"),
                (921, PipelineStatus::Success, "azure-devops-run-921"),
            ]
        );
        assert_eq!(runs[0].pipeline_id, "azure-devops__1__Alpha/1");
        assert_eq!(runs[1].branch.as_deref(), Some("main"));
        assert_eq!(runs[1].actor.as_deref(), Some("Ada Lovelace"));
        assert_eq!(runs[1].duration_seconds, Some(150));
        assert_eq!(runs[1].metadata["build_number"], json!("20261028.22"));

        let requests = stub.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].query("definitions").as_deref(), Some("1"));
        assert_eq!(
            requests[0].query("queryOrder").as_deref(),
            Some("queueTimeDescending")
        );
        assert_eq!(requests[0].query("$top").as_deref(), Some("3"));
        assert_eq!(requests[0].query("continuationToken"), None);
        assert_eq!(requests[1].query("$top").as_deref(), Some("1"));
        assert_eq!(
            requests[1].query("continuationToken").as_deref(),
            Some("builds-2")
        );
    }

    #[tokio::test]
    async fn run_history_stops_at_the_limit_without_following_the_continuation() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        let runs = plugin
            .fetch_run_history("azure-devops__1__Alpha/1", 1)
            .await
            .unwrap();

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_number, 923);
        assert_eq!(stub.requests().len(), 1);
        assert!(plugin
            .fetch_run_history("azure-devops__1__Alpha/1", 0)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(stub.requests().len(), 1);
    }

    #[tokio::test]
    async fn run_details_are_fetched_by_build_id_and_checked_against_the_pipeline() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        let run = plugin
            .fetch_run_details("azure-devops__1__Alpha/1", 922)
            .await
            .unwrap();
        assert_eq!(run.run_number, 922);
        assert_eq!(run.status, PipelineStatus::Failed);
        assert_eq!(run.commit_sha.as_deref(), Some("sha922"));
        assert_eq!(
            run.logs_url,
            "https://dev.azure.com/acme/Alpha/_build/results?buildId=922"
        );
        assert_eq!(run.inputs, Some(json!({"source-branch": "main"})));

        let foreign = plugin
            .fetch_run_details("azure-devops__1__Alpha/1", 777)
            .await
            .unwrap_err();
        assert!(
            matches!(foreign, PluginError::PipelineNotFound(_)),
            "{foreign}"
        );
        let missing = plugin
            .fetch_run_details("azure-devops__1__Alpha/1", 404)
            .await
            .unwrap_err();
        assert!(
            matches!(missing, PluginError::PipelineNotFound(_)),
            "{missing}"
        );
        let malformed = plugin
            .fetch_run_details("github__1__Alpha/1", 922)
            .await
            .unwrap_err();
        assert!(
            matches!(malformed, PluginError::InvalidConfig(_)),
            "{malformed}"
        );
    }

    #[tokio::test]
    async fn workflow_parameters_offer_the_default_branch_and_overridable_non_secret_variables() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        let parameters = plugin
            .fetch_workflow_parameters("azure-devops__1__Alpha/1")
            .await
            .unwrap();

        let summary: Vec<_> = parameters
            .iter()
            .map(|parameter| {
                let WorkflowParameterType::String { default } = &parameter.param_type else {
                    panic!("expected a string parameter: {}", parameter.name);
                };
                (
                    parameter.name.as_str(),
                    default.as_deref(),
                    parameter.required,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("source-branch", Some("main"), false),
                ("branch", Some("legacy"), false),
                ("deployTarget", Some("dev"), false),
            ]
        );
    }

    #[tokio::test]
    async fn trigger_queues_a_build_on_the_branch_with_the_overridden_variable() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        let result = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: "azure-devops__1__Alpha/1".to_string(),
                inputs: Some(json!({"source-branch": "feature/x", "deployTarget": "staging"})),
            })
            .await
            .unwrap();

        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            result,
            json!({
                "message": "Queued build 930 on feature/x",
                "run_number": 930,
                "logs_url": "https://dev.azure.com/acme/Alpha/_build/results?buildId=930"
            })
        );
        let requests = stub.requests();
        assert_eq!(requests.len(), 1);
        let queued = &requests[0];
        assert_eq!(queued.method, "POST");
        assert_eq!(queued.url.path(), "/acme/Alpha/_apis/build/builds");
        assert_eq!(queued.query("api-version").as_deref(), Some("7.0"));
        assert_eq!(queued.authorization.as_deref(), Some(BASIC_AUTH));
        let body: Value = serde_json::from_str(&queued.body).unwrap();
        assert_eq!(
            body,
            json!({
                "definition": {"id": 1},
                "sourceBranch": "refs/heads/feature/x",
                "parameters": r#"{"deployTarget":"staging"}"#
            })
        );
    }

    #[tokio::test]
    async fn a_variable_named_branch_is_sent_as_a_variable_not_as_the_source_branch() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: "azure-devops__1__Alpha/1".to_string(),
                inputs: Some(json!({"source-branch": "main", "branch": "legacy"})),
            })
            .await
            .unwrap();

        let body: Value = serde_json::from_str(&stub.requests()[0].body).unwrap();
        assert_eq!(
            body,
            json!({
                "definition": {"id": 1},
                "sourceBranch": "refs/heads/main",
                "parameters": r#"{"branch":"legacy"}"#
            })
        );
    }

    #[tokio::test]
    async fn cancel_patches_the_build_status_to_cancelling() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        plugin
            .cancel_run("azure-devops__1__Alpha/1", 923)
            .await
            .unwrap();

        let requests = stub.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].url.path(), "/acme/Alpha/_apis/build/builds/923");
        assert_eq!(requests[1].method, "PATCH");
        assert_eq!(requests[1].url.path(), "/acme/Alpha/_apis/build/builds/923");
        assert_eq!(requests[1].query("api-version").as_deref(), Some("7.0"));
        let body: Value = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body, json!({"status": "cancelling"}));
        assert!(plugin.metadata().capabilities.cancel);
    }

    #[tokio::test]
    async fn cancel_refuses_a_build_of_another_definition_without_patching() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "Alpha");

        let error = plugin
            .cancel_run("azure-devops__1__Alpha/1", 777)
            .await
            .unwrap_err();

        assert!(matches!(error, PluginError::PipelineNotFound(_)), "{error}");
        assert!(error.to_string().contains("does not belong"), "{error}");
        assert!(stub
            .requests()
            .iter()
            .all(|request| request.method == "GET"));
    }

    #[tokio::test]
    async fn ids_with_ci_keeps_projects_that_have_definitions() {
        let stub = serve(azure_reply).await;
        let plugin = plugin_for(&stub, "", "");

        let kept = plugin
            .ids_with_ci(vec!["Alpha".to_string(), "Empty".to_string()])
            .await
            .unwrap();

        assert_eq!(kept, ["Alpha"]);
    }

    #[tokio::test]
    async fn validate_accepts_a_working_token_with_a_single_cheap_call() {
        let stub = serve(|_| Reply::json(200, json!({"count": 0, "value": []}))).await;

        assert!(plugin_for(&stub, "", "")
            .validate_credentials()
            .await
            .unwrap());

        let requests = stub.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/acme/_apis/projects");
        assert_eq!(requests[0].query("$top").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn validate_maps_401_to_authentication_failed() {
        let stub =
            serve(|_| Reply::json(401, json!({"message": "TF400813: not authorized"}))).await;

        let error = plugin_for(&stub, "", "")
            .validate_credentials()
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::AuthenticationFailed(_)),
            "{error}"
        );
        assert_eq!(stub.requests().len(), 1);
    }

    #[tokio::test]
    async fn validate_maps_a_203_sign_in_page_to_authentication_failed() {
        let stub = serve(|_| Reply::html(203, "<html><title>Sign In</title></html>")).await;

        let error = plugin_for(&stub, "", "")
            .validate_credentials()
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::AuthenticationFailed(_)),
            "{error}"
        );
        assert!(error.to_string().contains("sign-in page"), "{error}");
        assert_eq!(stub.requests().len(), 1);
    }

    #[tokio::test]
    async fn validate_maps_a_200_html_sign_in_page_to_authentication_failed() {
        let stub = serve(|_| Reply::html(200, "<html><title>Sign In</title></html>")).await;

        let error = plugin_for(&stub, "", "")
            .validate_credentials()
            .await
            .unwrap_err();

        assert!(
            matches!(error, PluginError::AuthenticationFailed(_)),
            "{error}"
        );
    }

    #[tokio::test]
    async fn validate_maps_403_to_permission_denied() {
        let stub = serve(|_| Reply::json(403, json!({"message": "TF401444: no access"}))).await;

        let error = plugin_for(&stub, "", "")
            .validate_credentials()
            .await
            .unwrap_err();

        assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
    }

    #[test]
    fn initialize_requires_a_server_url_and_a_token() {
        let missing_url = AzureDevOpsPlugin::new()
            .initialize(
                1,
                HashMap::from([("token".to_string(), "t".to_string())]),
                None,
            )
            .unwrap_err();
        let missing_token = AzureDevOpsPlugin::new()
            .initialize(
                1,
                HashMap::from([(
                    "server_url".to_string(),
                    "https://dev.azure.com/acme".to_string(),
                )]),
                None,
            )
            .unwrap_err();

        assert!(matches!(missing_url, PluginError::InvalidConfig(_)));
        assert!(matches!(missing_token, PluginError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn a_list_cut_short_by_the_page_cap_is_an_error() {
        let stub = serve(|_| {
            Reply::json(
                200,
                json!({"count": 1, "value": [{"id": "p", "name": "Alpha"}]}),
            )
            .continued("again")
        })
        .await;

        let error = plugin_for(&stub, "", "")
            .fetch_available_pipelines(None)
            .await
            .unwrap_err();

        assert!(matches!(error, PluginError::ApiError(_)), "{error}");
        assert!(error.to_string().contains("more items"), "{error}");
        assert!(stub.requests().len() > 1);
    }
}
