use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{
    TimeZone,
    Utc,
};
use pipedash_plugin_api::http::{
    check_response,
    decode_json,
};
use pipedash_plugin_api::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct StubRun {
    number: i64,
    status: PipelineStatus,
    #[serde(default)]
    timestamp: i64,
    #[serde(default)]
    duration: i64,
}

#[derive(Deserialize)]
struct StubRuns {
    runs: Vec<StubRun>,
}

pub(super) struct StubPlugin {
    metadata: PluginMetadata,
    url: String,
    token: String,
    client: Option<Arc<reqwest::Client>>,
}

impl StubPlugin {
    pub(super) fn new() -> Self {
        let capabilities = PluginCapabilities {
            trigger: true,
            cancel: true,
            logs: true,
            uses_http_proxy: true,
            ..PluginCapabilities::default()
        };

        Self {
            metadata: PluginMetadata::new(
                "Stub",
                ConfigSchema::new(),
                pipedash_plugin_api::defaults::table_schema(
                    pipedash_plugin_api::defaults::default_pipeline_runs_table(),
                    pipedash_plugin_api::defaults::default_pipelines_table(),
                ),
                capabilities,
            ),
            url: String::new(),
            token: String::new(),
            client: None,
        }
    }

    fn pipeline_id(&self) -> String {
        "build".to_string()
    }

    fn request(
        &self, method: reqwest::Method, path: &str,
    ) -> PluginResult<reqwest::RequestBuilder> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("HTTP client missing".into()))?;

        Ok(client
            .request(method, format!("{}{path}", self.url))
            .bearer_auth(&self.token))
    }

    async fn send(&self, method: reqwest::Method, path: &str) -> PluginResult<reqwest::Response> {
        let response = self
            .request(method, path)?
            .send()
            .await
            .map_err(|error| PluginError::NetworkError(error.to_string()))?;

        check_response(response).await
    }

    fn run(&self, raw: StubRun) -> PipelineRun {
        PipelineRun {
            id: format!("stub-run-{}", raw.number),
            pipeline_id: self.pipeline_id(),
            run_number: raw.number,
            status: raw.status,
            started_at: Utc
                .timestamp_millis_opt(raw.timestamp)
                .single()
                .unwrap_or_default(),
            concluded_at: None,
            duration_seconds: Some(raw.duration / 1000),
            logs_url: String::new(),
            commit_sha: None,
            commit_message: None,
            branch: None,
            actor: None,
            inputs: None,
            metadata: HashMap::new(),
        }
    }

    async fn runs(&self, limit: usize) -> PluginResult<Vec<PipelineRun>> {
        let response = self
            .send(reqwest::Method::GET, &format!("/runs?limit={limit}"))
            .await?;
        let runs: StubRuns = decode_json(response, "stub runs").await?;

        Ok(runs.runs.into_iter().map(|raw| self.run(raw)).collect())
    }
}

#[async_trait]
impl Plugin for StubPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        self.url = config.get("server_url").cloned().unwrap_or_default();
        self.token = config.get("token").cloned().unwrap_or_default();
        self.client = http_client;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        Ok(true)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let latest = self.runs(1).await?.into_iter().next();

        Ok(vec![Pipeline {
            id: self.pipeline_id(),
            name: "build".into(),
            status: latest
                .as_ref()
                .map_or(PipelineStatus::Pending, |run| run.status),
            last_run: latest.map(|run| run.started_at),
            last_updated: Utc::now(),
            repository: String::new(),
            branch: None,
            workflow_file: None,
            resource_state: None,
            metadata: HashMap::new(),
        }])
    }

    async fn fetch_run_history(
        &self, _pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        self.runs(limit).await
    }

    async fn fetch_run_details(
        &self, _pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let response = self
            .send(reqwest::Method::GET, &format!("/runs/{run_number}"))
            .await?;

        Ok(self.run(decode_json(response, "stub run").await?))
    }

    async fn trigger_pipeline(&self, _params: TriggerParams) -> PluginResult<TriggerResult> {
        self.send(reqwest::Method::POST, "/trigger").await?;

        Ok(TriggerResult::default())
    }

    async fn cancel_run(&self, _pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        self.send(reqwest::Method::POST, &format!("/runs/{run_number}/cancel"))
            .await
            .map(drop)
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        self.send(reqwest::Method::GET, "/params").await?;

        Ok(Vec::new())
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &pipedash_plugin_actions::ActionsLogDialect
    }

    async fn fetch_log_nodes(&self, _run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        Ok(vec![LogNode {
            id: "501".into(),
            parent_id: None,
            kind: LogNodeKind::Job,
            name: "build".into(),
            status: PipelineStatus::Success,
            started_at: None,
            finished_at: None,
            log: Some(LogAvailability::Ready),
            web_url: None,
        }])
    }

    async fn read_log(
        &self, _run: RunRef<'_>, node_id: &str, _from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let response = self
            .send(reqwest::Method::GET, &format!("/logs/{node_id}"))
            .await?;
        sink.pipe(response).await?;

        Ok(LogRead::replace(None))
    }
}
