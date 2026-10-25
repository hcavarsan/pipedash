use std::collections::HashMap;

use async_trait::async_trait;
use futures::future::join_all;
use octocrab::service::middleware::retry::RetryConfig;
use octocrab::Octocrab;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    metadata,
};

pub struct GitHubPlugin {
    metadata: PluginMetadata,
    client: Option<client::GitHubClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    status_source: Option<StatusSource>,
}

impl Default for GitHubPlugin {
    fn default() -> Self {
        Self::new()
    }
}

mod permission_mapping {

    pub const FINE_GRAINED_FEATURE_MAPPINGS: &[(&str, &str)] = &[
        ("list_monitor_workflows", "Repository Metadata"),
        ("view_run_history", "Repository Metadata"),
        ("monitor_status", "Actions (Read)"),
        ("trigger_dispatch", "Actions (Write)"),
        ("cancel_workflows", "Actions (Write)"),
        ("rerun_failed_jobs", "Actions (Write)"),
        ("access_org_repos", "Organization members and teams (Read)"),
    ];

    pub fn map_feature_permissions(
        token_type: &str, feature_id: &str, classic_permissions: &[String],
    ) -> Vec<String> {
        match token_type {
            "fine_grained" => FINE_GRAINED_FEATURE_MAPPINGS
                .iter()
                .find(|(id, _)| *id == feature_id)
                .map(|(_, perm)| vec![perm.to_string()])
                .unwrap_or_default(),
            _ => classic_permissions.to_vec(),
        }
    }
}

impl GitHubPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            status_source: None,
        }
    }

    fn client(&self) -> PluginResult<&client::GitHubClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }
}

#[async_trait]
impl Plugin for GitHubPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing GitHub token".to_string()))?;

        if token.is_empty() {
            tracing::error!(provider_id = provider_id, "GitHub token is empty");
            return Err(PluginError::InvalidConfig(
                "GitHub token is empty. Please check keyring permissions.".to_string(),
            ));
        }

        tracing::debug!(token_length = token.len(), "Initializing GitHub plugin");

        let base_url = config::get_base_url(&config);
        let api_url = config::build_api_url(&base_url)?;
        let status_source = config::status_source(&config, &base_url)?;

        tracing::debug!(api_url = %api_url, "Using GitHub API URL");

        let octocrab = Octocrab::builder()
            .add_retry_config(RetryConfig::None)
            .personal_token(token.clone())
            .base_uri(&api_url)
            .map_err(|e| PluginError::InvalidConfig(format!("Failed to set base URI: {e}")))?
            .build()
            .map_err(|e| {
                PluginError::InvalidConfig(format!("Failed to build GitHub client: {e}"))
            })?;

        let github_client = client::GitHubClient::new(octocrab, token.clone(), &api_url)?;
        self.client = Some(github_client);
        self.provider_id = Some(provider_id);
        self.config = config;
        self.status_source = status_source;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        client
            .retry_policy
            .retry(|| async {
                client
                    .octocrab
                    .current()
                    .user()
                    .await
                    .map(|_| true)
                    .map_err(|e| client::map_octocrab_error("Failed to validate credentials", e))
            })
            .await
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    async fn fetch_rate_limit(&self) -> PluginResult<Option<RateLimit>> {
        self.client()?.fetch_rate_limit().await.map(Some)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        client.fetch_all_repositories(params).await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let client = self.client()?;
        client.fetch_organizations().await
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        client
            .fetch_available_pipelines_filtered(org, search, params)
            .await
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let repositories = config::get_repositories(&self.config);
        tracing::debug!(repositories = ?repositories, "Configured GitHub repositories");

        if repositories.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No repositories configured".to_string(),
            ));
        }

        let client = self.client()?;
        let futures = repositories
            .iter()
            .map(|repo_full_name| client.fetch_repo_workflows(provider_id, repo_full_name.clone()));

        let results = join_all(futures).await;

        let mut all_pipelines = Vec::new();
        let mut first_error = None;

        for (repo_full_name, result) in repositories.iter().zip(results) {
            match result {
                Ok(mut pipelines) => {
                    tracing::debug!(count = pipelines.len(), "GitHub repo returned workflows");
                    all_pipelines.append(&mut pipelines);
                }
                Err(e) if e.is_transient() => return Err(e),
                Err(e) => {
                    tracing::warn!(
                        repository = %repo_full_name,
                        error = %e,
                        "Skipping GitHub repository after a non-transient failure"
                    );
                    first_error.get_or_insert(e);
                }
            }
        }

        let unique_count = all_pipelines
            .iter()
            .map(|p| &p.id)
            .collect::<std::collections::HashSet<_>>()
            .len();
        tracing::debug!(
            unique_pipelines = unique_count,
            total_pipelines = all_pipelines.len(),
            "GitHub pipeline fetch complete"
        );

        match first_error {
            Some(e) if all_pipelines.is_empty() => Err(e),
            _ => Ok(all_pipelines),
        }
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let runs = client
            .fetch_run_history(owner, repo, workflow_id, limit)
            .await?;

        let pipeline_runs = runs
            .into_iter()
            .map(|run| client::run_to_pipeline_run(run, pipeline_id))
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        Ok(client::run_to_pipeline_run(run, pipeline_id))
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let parts: Vec<&str> = workflow_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {} (expected 5 parts, got {})",
                workflow_id,
                parts.len()
            )));
        }

        let workflow_number_str = parts[4];
        let workflow_number: u64 = workflow_number_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_number_str}"))
        })?;

        let source = self
            .client()?
            .fetch_workflow_source(parts[2], parts[3], workflow_number)
            .await?;

        let mut parameters = vec![WorkflowParameter {
            name: "ref".to_string(),
            label: Some("Ref".to_string()),
            description: Some("Branch, tag, or commit SHA to run workflow on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }];
        parameters.extend(workflow_dispatch::dispatch_inputs(&source)?);
        Ok(parameters)
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let parts: Vec<&str> = params.workflow_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {} (expected 5 parts, got {})",
                params.workflow_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id = parts[4];

        let body = workflow_dispatch::dispatch_body(params.inputs);
        let ref_value = body["ref"].as_str().unwrap_or("main").to_string();

        let client = self.client()?;
        let url = format!("/repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches");

        let response: Result<serde_json::Value, octocrab::Error> =
            client.octocrab.post(url, Some(&body)).await;

        if let Err(e) = response {
            return Err(client::map_octocrab_error("Failed to trigger workflow", e));
        }

        let workflow_id_u64: u64 = workflow_id.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id}"))
        })?;

        let trigger_time = chrono::Utc::now();

        let previous_runs = client
            .fetch_run_history(owner, repo, workflow_id_u64, 5)
            .await?;
        let previous_latest_run_number = previous_runs.first().map(|r| r.run_number).unwrap_or(0);

        let mut new_run = None;
        for attempt in 1..=10 {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

            let runs = client
                .fetch_run_history(owner, repo, workflow_id_u64, 5)
                .await?;

            for run in runs {
                if run.run_number > previous_latest_run_number
                    && run.head_branch == ref_value
                    && run.created_at.with_timezone(&chrono::Utc) >= trigger_time
                {
                    tracing::debug!(
                        run_number = run.run_number,
                        attempt = attempt,
                        "Found new GitHub run"
                    );
                    new_run = Some(run);
                    break;
                }
            }

            if new_run.is_some() {
                break;
            }

            tracing::debug!(
                attempt = attempt,
                max_attempts = 10,
                "Waiting for new GitHub run to appear"
            );
        }

        let (logs_url, run_number) = if let Some(run) = new_run {
            (run.html_url.to_string(), run.run_number)
        } else {
            let runs = client
                .fetch_run_history(owner, repo, workflow_id_u64, 1)
                .await?;
            (
                runs.first()
                    .map(|r| r.html_url.to_string())
                    .unwrap_or_default(),
                runs.first().map(|r| r.run_number).unwrap_or(0),
            )
        };

        Ok(serde_json::json!({
            "message": format!("Triggered workflow on ref {}", ref_value),
            "run_number": run_number,
            "logs_url": logs_url
        })
        .to_string())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        let run_id_u64: u64 = run.id.0;
        client.cancel_run(owner, repo, run_id_u64).await
    }

    async fn review_run(
        &self, pipeline_id: &str, run_number: i64, decision: ReviewDecision,
    ) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        client
            .review_pending_deployments(owner, repo, run.id.0, run_number, decision)
            .await
    }

    async fn rerun_failed_jobs(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        client.rerun_failed_jobs(owner, repo, run.id.0).await
    }

    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        let client = self.client()?;
        client.check_token_permissions().await
    }

    fn get_feature_availability(&self, status: &PermissionStatus) -> Vec<FeatureAvailability> {
        use permission_mapping::map_feature_permissions;

        let features = &self.metadata().features;

        let token_type = status
            .metadata
            .get("token_type")
            .map(|s| s.as_str())
            .unwrap_or("classic_pat");

        let granted_perms: std::collections::HashSet<String> = status
            .permissions
            .iter()
            .filter(|p| p.granted)
            .map(|p| p.permission.name.clone())
            .collect();

        features
            .iter()
            .map(|feature| {
                let mapped_required =
                    map_feature_permissions(token_type, &feature.id, &feature.required_permissions);

                let missing: Vec<String> = mapped_required
                    .iter()
                    .filter(|perm| !granted_perms.contains(*perm))
                    .cloned()
                    .collect();

                let transformed_feature = Feature {
                    id: feature.id.clone(),
                    name: feature.name.clone(),
                    description: feature.description.clone(),
                    required_permissions: mapped_required,
                };

                FeatureAvailability {
                    feature: transformed_feature,
                    available: missing.is_empty(),
                    missing_permissions: missing,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::sync::Mutex;

    use super::*;

    const PIPELINE_ID: &str = "github__1__acme__app__42";
    const PENDING_PATH: &str = "/api/v3/repos/acme/app/actions/runs/9001/pending_deployments";

    type Recorded = Arc<Mutex<Vec<(String, String, String)>>>;

    fn waiting_run() -> serde_json::Value {
        let url = "http://stub.invalid/acme/app";
        serde_json::json!({
            "id": 9001, "workflow_id": 42, "node_id": "n", "name": "Deploy",
            "head_branch": "main", "head_sha": "abc", "run_number": 7, "event": "push",
            "status": "waiting", "conclusion": null,
            "created_at": "2026-10-20T10:00:00Z", "updated_at": "2026-10-20T10:01:00Z",
            "url": url, "html_url": url, "jobs_url": url, "logs_url": url,
            "check_suite_url": url, "artifacts_url": url, "cancel_url": url,
            "rerun_url": url, "workflow_url": url,
            "head_commit": {
                "id": "abc", "tree_id": "def", "message": "ship it",
                "timestamp": "2026-10-20T09:59:00Z",
                "author": {"name": "Ada", "email": "ada@example.com"},
                "committer": {"name": "Ada", "email": "ada@example.com"}
            },
            "repository": {"id": 1, "name": "app", "url": url}
        })
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, String, String) {
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        let head_end = loop {
            if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
            let count = stream.read(&mut buffer).await.unwrap();
            assert_ne!(count, 0, "stub client closed early");
            request.extend_from_slice(&buffer[..count]);
        };
        let head = String::from_utf8(request[..head_end].to_vec()).unwrap();
        let length: usize = head
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::to_string)
            })
            .map(|value| value.trim().parse().unwrap())
            .unwrap_or(0);
        while request.len() < head_end + length {
            let count = stream.read(&mut buffer).await.unwrap();
            request.extend_from_slice(&buffer[..count]);
        }
        let body = String::from_utf8(request[head_end..head_end + length].to_vec()).unwrap();
        let mut first_line = head.split_whitespace();
        let method = first_line.next().unwrap().to_string();
        let target = first_line.next().unwrap();
        let path = target.split('?').next().unwrap().to_string();
        (method, path, body)
    }

    struct Reply {
        status: u16,
        headers: Vec<(&'static str, &'static str)>,
        body: serde_json::Value,
    }

    async fn serve(
        config: &[(&str, &str)], handler: impl Fn(&str, &str) -> Reply + Send + 'static,
    ) -> (GitHubPlugin, Recorded, tokio::task::JoinHandle<()>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let recorded: Recorded = Arc::default();
        let log = recorded.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (method, path, body) = read_request(&mut stream).await;
                let reply = handler(&method, &path);
                log.lock().await.push((method, path, body));
                let body = match reply.body {
                    serde_json::Value::String(text) => text,
                    json => json.to_string(),
                };
                let extra: String = reply
                    .headers
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}\r\n"))
                    .collect();
                let response = format!(
                    "HTTP/1.1 {} Stub\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    reply.status,
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let mut plugin = GitHubPlugin::new();
        let mut settings = config_with(config);
        settings.insert("base_url".to_string(), format!("http://{addr}"));
        plugin.initialize(1, settings, None).unwrap();
        (plugin, recorded, server)
    }

    async fn stub_github(
        pending_deployments: serde_json::Value,
    ) -> (GitHubPlugin, Recorded, tokio::task::JoinHandle<()>) {
        serve(&[], move |method, path| match (method, path) {
            ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => {
                ok(serde_json::json!({"total_count": 1, "workflow_runs": [waiting_run()]}))
            }
            ("GET", PENDING_PATH) => ok(pending_deployments.clone()),
            ("POST", PENDING_PATH) => ok(serde_json::json!([])),
            (method, path) => panic!("Unexpected stub request: {method} {path}"),
        })
        .await
    }

    fn pending(id: u64, name: &str, can_approve: bool) -> serde_json::Value {
        serde_json::json!({
            "environment": {"id": id, "name": name},
            "wait_timer": 0,
            "current_user_can_approve": can_approve,
            "reviewers": []
        })
    }

    async fn posts(recorded: &Recorded) -> Vec<serde_json::Value> {
        recorded
            .lock()
            .await
            .iter()
            .filter(|(method, _, _)| method == "POST")
            .map(|(_, _, body)| serde_json::from_str(body).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn approve_posts_only_environments_the_token_can_approve() {
        let (plugin, recorded, server) = stub_github(serde_json::json!([
            pending(11, "production", true),
            pending(12, "staging", true),
            pending(13, "audit", false),
        ]))
        .await;

        plugin
            .review_run(PIPELINE_ID, 7, ReviewDecision::Approve)
            .await
            .unwrap();

        assert_eq!(
            posts(&recorded).await,
            [serde_json::json!({
                "environment_ids": [11, 12],
                "state": "approved",
                "comment": "Approved in Pipedash"
            })]
        );
        server.abort();
    }

    #[tokio::test]
    async fn reject_posts_rejected_state() {
        let (plugin, recorded, server) =
            stub_github(serde_json::json!([pending(11, "production", true)])).await;

        plugin
            .review_run(PIPELINE_ID, 7, ReviewDecision::Reject)
            .await
            .unwrap();

        assert_eq!(
            posts(&recorded).await,
            [serde_json::json!({
                "environment_ids": [11],
                "state": "rejected",
                "comment": "Rejected in Pipedash"
            })]
        );
        server.abort();
    }

    #[tokio::test]
    async fn review_fails_without_posting_when_token_can_approve_none() {
        let (plugin, recorded, server) = stub_github(serde_json::json!([
            pending(11, "production", false),
            pending(12, "staging", false),
        ]))
        .await;

        let error = plugin
            .review_run(PIPELINE_ID, 7, ReviewDecision::Approve)
            .await
            .unwrap_err();

        assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
        assert!(error.to_string().contains("production, staging"), "{error}");
        assert!(posts(&recorded).await.is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn review_fails_without_posting_when_run_waits_for_nothing() {
        let (plugin, recorded, server) = stub_github(serde_json::json!([])).await;

        let error = plugin
            .review_run(PIPELINE_ID, 7, ReviewDecision::Approve)
            .await
            .unwrap_err();

        assert!(matches!(error, PluginError::PipelineNotFound(_)), "{error}");
        assert!(posts(&recorded).await.is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn waiting_run_is_pending_and_flagged_for_review() {
        let (plugin, _recorded, server) = stub_github(serde_json::json!([])).await;

        let run = plugin.fetch_run_details(PIPELINE_ID, 7).await.unwrap();

        assert_eq!(run.status, PipelineStatus::Pending);
        assert_eq!(
            run.metadata.get("awaiting_review"),
            Some(&serde_json::json!(true))
        );
        server.abort();
    }

    fn finished_run(
        id: u64, run_number: i64, status: &str, conclusion: Option<&str>,
    ) -> serde_json::Value {
        let mut run = waiting_run();
        run["id"] = serde_json::json!(id);
        run["run_number"] = serde_json::json!(run_number);
        run["status"] = serde_json::json!(status);
        run["conclusion"] = serde_json::json!(conclusion);
        run
    }

    async fn stub_failed_runs(
        rerun_reply: fn() -> Reply,
    ) -> (GitHubPlugin, Recorded, tokio::task::JoinHandle<()>) {
        serve(&[], move |method, path| match (method, path) {
            ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => ok(serde_json::json!({
                "total_count": 2,
                "workflow_runs": [
                    finished_run(9002, 8, "completed", Some("failure")),
                    finished_run(9001, 7, "completed", Some("failure")),
                ]
            })),
            ("POST", "/api/v3/repos/acme/app/actions/runs/9001/rerun-failed-jobs") => rerun_reply(),
            (method, path) => panic!("Unexpected stub request: {method} {path}"),
        })
        .await
    }

    #[tokio::test]
    async fn rerun_failed_jobs_posts_for_the_run_with_that_number() {
        let (plugin, recorded, server) = stub_failed_runs(|| Reply {
            status: 201,
            headers: Vec::new(),
            body: serde_json::json!({}),
        })
        .await;

        plugin.rerun_failed_jobs(PIPELINE_ID, 7).await.unwrap();

        let posted: Vec<String> = recorded
            .lock()
            .await
            .iter()
            .filter(|(method, _, _)| method == "POST")
            .map(|(_, path, _)| path.clone())
            .collect();
        assert_eq!(
            posted,
            ["/api/v3/repos/acme/app/actions/runs/9001/rerun-failed-jobs"]
        );
        server.abort();
    }

    #[tokio::test]
    async fn rerun_failed_jobs_returns_the_github_refusal() {
        let (plugin, _recorded, server) = stub_failed_runs(|| {
            failure(
                403,
                "Unable to retry this workflow run because it was created over a month ago",
                &[],
            )
        })
        .await;

        let error = plugin.rerun_failed_jobs(PIPELINE_ID, 7).await.unwrap_err();

        assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
        assert!(error.to_string().contains("over a month ago"), "{error}");
        server.abort();
    }

    #[test]
    fn only_failed_and_timed_out_runs_offer_a_failed_jobs_rerun() {
        let cases = [
            ("completed", Some("failure"), true),
            ("completed", Some("timed_out"), true),
            ("completed", Some("success"), false),
            ("completed", Some("cancelled"), false),
            ("completed", Some("startup_failure"), false),
            ("completed", Some("action_required"), false),
            ("in_progress", None, false),
            ("waiting", None, false),
        ];
        for (status, conclusion, expected) in cases {
            let run: crate::types::Run =
                serde_json::from_value(finished_run(9001, 7, status, conclusion)).unwrap();

            let run = client::run_to_pipeline_run(run, PIPELINE_ID);

            assert_eq!(
                run.metadata.get("can_rerun_failed_jobs") == Some(&serde_json::json!(true)),
                expected,
                "{status} {conclusion:?}"
            );
        }
    }

    fn failure(status: u16, message: &str, headers: &[(&'static str, &'static str)]) -> Reply {
        Reply {
            status,
            headers: headers.to_vec(),
            body: serde_json::json!({"message": message}),
        }
    }

    fn ok(body: serde_json::Value) -> Reply {
        Reply {
            status: 200,
            headers: Vec::new(),
            body,
        }
    }

    fn workflow(id: u64) -> serde_json::Value {
        let url = "http://stub.invalid/acme/app";
        serde_json::json!({
            "id": id, "node_id": "n", "name": "CI", "path": ".github/workflows/ci.yml",
            "state": "active", "created_at": "2026-10-20T10:00:00Z",
            "updated_at": "2026-10-20T10:00:00Z", "url": url, "html_url": url, "badge_url": url
        })
    }

    fn workflows_of_ok_repo() -> Reply {
        ok(serde_json::json!({"total_count": 1, "workflows": [workflow(42)]}))
    }

    fn runs_of_ok_repo() -> Reply {
        ok(serde_json::json!({"total_count": 1, "workflow_runs": [waiting_run()]}))
    }

    fn rate_limited() -> Reply {
        failure(429, "API rate limit exceeded", &[("Retry-After", "120")])
    }

    #[tokio::test]
    async fn rate_limited_response_is_sent_once_and_keeps_the_hint() {
        let (plugin, recorded, server) = serve(&[], |_, _| rate_limited()).await;

        let error = plugin.validate_credentials().await.unwrap_err();

        assert!(
            matches!(
                error,
                PluginError::RateLimited { retry_after: Some(hint), .. }
                    if hint == Duration::from_secs(120)
            ),
            "{error}"
        );
        assert_eq!(recorded.lock().await.len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn server_errors_are_retried_by_the_retry_policy_only() {
        let (plugin, recorded, server) =
            serve(&[], |_, _| failure(503, "Service Unavailable", &[])).await;

        let error = plugin.validate_credentials().await.unwrap_err();

        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error}"
        );
        assert_eq!(
            recorded.lock().await.len(),
            RetryPolicy::default().max_retries
        );
        server.abort();
    }

    #[tokio::test]
    async fn run_lookups_and_repository_lists_use_the_retry_policy() {
        let attempts = RetryPolicy::default().max_retries;
        let (plugin, recorded, server) =
            serve(&[], |_, _| failure(503, "Service Unavailable", &[])).await;

        let error = plugin.fetch_run_details(PIPELINE_ID, 7).await.unwrap_err();
        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error}"
        );
        assert_eq!(recorded.lock().await.len(), attempts);

        let error = plugin.fetch_available_pipelines(None).await.unwrap_err();
        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error}"
        );
        assert_eq!(recorded.lock().await.len(), attempts * 2);

        let error = plugin.fetch_run_history(PIPELINE_ID, 5).await.unwrap_err();
        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error}"
        );
        assert_eq!(recorded.lock().await.len(), attempts * 3);
        server.abort();
    }

    #[tokio::test]
    async fn bad_credentials_are_authentication_failures_without_retry() {
        let (plugin, recorded, server) =
            serve(&[], |_, _| failure(401, "Bad credentials", &[])).await;

        let error = plugin.validate_credentials().await.unwrap_err();

        assert!(
            matches!(error, PluginError::AuthenticationFailed(_)),
            "{error}"
        );
        assert_eq!(recorded.lock().await.len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn forbidden_is_rate_limited_only_when_github_says_so() {
        let (plugin, _recorded, server) = serve(&[], |_, _| {
            failure(
                403,
                "You have exceeded a secondary rate limit.",
                &[("Retry-After", "120")],
            )
        })
        .await;
        let error = plugin.validate_credentials().await.unwrap_err();
        assert!(matches!(error, PluginError::RateLimited { .. }), "{error}");
        server.abort();

        let (plugin, _recorded, server) =
            serve(&[], |_, _| failure(403, "Resource not accessible", &[])).await;
        let error = plugin.validate_credentials().await.unwrap_err();
        assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
        server.abort();
    }

    #[tokio::test]
    async fn rate_limit_on_one_repo_fails_the_whole_fetch() {
        let (plugin, _recorded, server) = serve(
            &[("selected_items", "acme/ok,acme/limited")],
            |_, path| match path {
                "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
                "/api/v3/repos/acme/ok/actions/workflows/42/runs" => runs_of_ok_repo(),
                "/api/v3/repos/acme/limited/actions/workflows" => rate_limited(),
                path => panic!("Unexpected stub request: {path}"),
            },
        )
        .await;

        let error = plugin.fetch_pipelines().await.unwrap_err();

        assert!(matches!(error, PluginError::RateLimited { .. }), "{error}");
        server.abort();
    }

    #[tokio::test]
    async fn missing_repo_is_skipped_while_the_others_are_returned() {
        let (plugin, _recorded, server) = serve(
            &[("selected_items", "acme/ok,acme/gone")],
            |_, path| match path {
                "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
                "/api/v3/repos/acme/ok/actions/workflows/42/runs" => runs_of_ok_repo(),
                "/api/v3/repos/acme/gone/actions/workflows" => failure(404, "Not Found", &[]),
                path => panic!("Unexpected stub request: {path}"),
            },
        )
        .await;

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].repository, "acme/ok");
        server.abort();
    }

    #[tokio::test]
    async fn rate_limit_on_latest_run_fails_instead_of_reporting_pending() {
        let (plugin, _recorded, server) =
            serve(&[("selected_items", "acme/ok")], |_, path| match path {
                "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
                "/api/v3/repos/acme/ok/actions/workflows/42/runs" => rate_limited(),
                path => panic!("Unexpected stub request: {path}"),
            })
            .await;

        let error = plugin.fetch_pipelines().await.unwrap_err();

        assert!(matches!(error, PluginError::RateLimited { .. }), "{error}");
        server.abort();
    }

    #[tokio::test]
    async fn permission_check_uses_the_configured_api_url() {
        let (plugin, recorded, server) = serve(&[], |method, path| match (method, path) {
            ("GET", "/api/v3/user") => Reply {
                status: 200,
                headers: vec![("X-OAuth-Scopes", "repo, workflow, read:org")],
                body: serde_json::json!({}),
            },
            (method, path) => panic!("Unexpected stub request: {method} {path}"),
        })
        .await;

        let status = plugin.check_permissions().await.unwrap();

        assert_eq!(
            status.metadata.get("token_type").map(String::as_str),
            Some("classic_pat")
        );
        assert!(status.all_granted);
        assert_eq!(recorded.lock().await.len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn workflow_parameters_list_the_workflow_dispatch_inputs_after_ref() {
        let source = "on:\n  push:\n  workflow_dispatch:\n    inputs:\n      environment:\n        description: Target\n        required: true\n        type: choice\n        options: [staging, production]\n      ref:\n        default: v1\n";
        let (plugin, recorded, server) = serve(&[], move |method, path| match (method, path) {
            ("GET", "/api/v3/repos/acme/app/actions/workflows/42") => ok(workflow(42)),
            ("GET", "/api/v3/repos/acme/app/contents/.github/workflows/ci.yml") => {
                ok(serde_json::json!(source))
            }
            (method, path) => panic!("Unexpected stub request: {method} {path}"),
        })
        .await;

        let params = plugin.fetch_workflow_parameters(PIPELINE_ID).await.unwrap();
        let summary: Vec<_> = params
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.required,
                    serde_json::to_value(&p.param_type).unwrap(),
                )
            })
            .collect();

        assert_eq!(
            summary,
            [
                (
                    "ref",
                    true,
                    serde_json::json!({"type": "string", "default": "main"})
                ),
                (
                    "inputs.environment",
                    true,
                    serde_json::json!({"type": "choice", "options": ["staging", "production"], "default": null})
                ),
                (
                    "inputs.ref",
                    false,
                    serde_json::json!({"type": "string", "default": "v1"})
                ),
            ]
        );
        assert_eq!(params[1].label.as_deref(), Some("environment"));
        assert_eq!(recorded.lock().await.len(), 2);
        server.abort();
    }

    fn config_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        let mut config = HashMap::from([("token".to_string(), "stub-token".to_string())]);
        config.extend(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        config
    }

    #[tokio::test]
    async fn status_source_is_recomputed_on_every_initialize() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut plugin = GitHubPlugin::new();
        assert!(plugin.status_source().is_none());

        plugin
            .initialize(1, config_with(&[("base_url", "https://github.com")]), None)
            .unwrap();
        assert_eq!(plugin.status_source().unwrap().name, "GitHub");

        plugin
            .initialize(
                1,
                config_with(&[("base_url", "https://github.enterprise.com")]),
                None,
            )
            .unwrap();
        assert!(plugin.status_source().is_none());

        plugin
            .initialize(
                1,
                config_with(&[
                    ("base_url", "https://github.enterprise.com"),
                    ("status_page_url", "https://status.example.com"),
                ]),
                None,
            )
            .unwrap();
        assert_eq!(plugin.status_source().unwrap().name, "status.example.com");
    }

    #[test]
    fn invalid_status_page_url_fails_initialize() {
        let mut plugin = GitHubPlugin::new();

        let error = plugin
            .initialize(
                1,
                config_with(&[("status_page_url", "javascript:alert(1)")]),
                None,
            )
            .unwrap_err();

        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error}");
    }
}
