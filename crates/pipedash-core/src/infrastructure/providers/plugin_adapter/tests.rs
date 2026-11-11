use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
};

use async_trait::async_trait;
use chrono::Utc;
use pipedash_plugin_api::defaults::{
    default_pipeline_runs_table,
    default_pipelines_table,
    table_schema,
};
use pipedash_plugin_api::{
    ConfigSchema,
    LogNode,
    Pipeline as ApiPipeline,
    PipelineRun as ApiRun,
    PipelineStatus as ApiStatus,
    Plugin,
    PluginCapabilities,
    PluginMetadata,
    PluginResult,
    ReviewDecision,
    RunRef,
    TriggerParams as ApiTriggerParams,
    TriggerResult,
    WorkflowParameter,
};

use super::*;
use crate::domain::{
    DomainError,
    Provider,
};

const TYPE: &str = "recorder";
const PROVIDER: i64 = 3;
const LOCAL: &str = "owner/repo/ci.yml";
const GLOBAL: &str = "recorder__3__owner/repo/ci.yml";

type Calls = Arc<Mutex<Vec<String>>>;

struct Recorder {
    metadata: PluginMetadata,
    calls: Calls,
}

impl Recorder {
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn run(pipeline_id: &str) -> ApiRun {
        ApiRun {
            id: "run-9".into(),
            pipeline_id: pipeline_id.into(),
            run_number: 9,
            status: ApiStatus::Success,
            started_at: Utc::now(),
            concluded_at: None,
            duration_seconds: None,
            logs_url: String::new(),
            commit_sha: None,
            commit_message: None,
            branch: None,
            actor: None,
            inputs: None,
            metadata: HashMap::new(),
            correlation: Default::default(),
            evidence: Default::default(),
        }
    }
}

#[async_trait]
impl Plugin for Recorder {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, _provider_id: i64, _config: HashMap<String, String>,
        _http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        Ok(true)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<ApiPipeline>> {
        Ok(vec![ApiPipeline {
            id: LOCAL.into(),
            name: "ci".into(),
            status: ApiStatus::Success,
            last_run: None,
            last_updated: Utc::now(),
            repository: "owner/repo".into(),
            branch: None,
            workflow_file: None,
            resource_state: None,
            metadata: HashMap::new(),
            correlation: Default::default(),
        }])
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, _limit: usize,
    ) -> PluginResult<Vec<ApiRun>> {
        self.record(format!("history:{pipeline_id}"));
        Ok(vec![Self::run(pipeline_id)])
    }

    async fn fetch_run_details(&self, pipeline_id: &str, _number: i64) -> PluginResult<ApiRun> {
        self.record(format!("details:{pipeline_id}"));
        Ok(Self::run(pipeline_id))
    }

    async fn fetch_recent_runs(
        &self, pipeline_ids: &[String], _limit: usize,
    ) -> PluginResult<HashMap<String, Vec<ApiRun>>> {
        self.record(format!("recent:{}", pipeline_ids.join(",")));
        Ok(pipeline_ids
            .iter()
            .map(|id| (id.clone(), vec![Self::run(id)]))
            .collect())
    }

    async fn trigger_pipeline(&self, params: ApiTriggerParams) -> PluginResult<TriggerResult> {
        self.record(format!("trigger:{}", params.workflow_id));
        Ok(TriggerResult {
            run_id: Some("run-10".into()),
            run_number: Some(10),
        })
    }

    async fn cancel_run(&self, pipeline_id: &str, _number: i64) -> PluginResult<()> {
        self.record(format!("cancel:{pipeline_id}"));
        Ok(())
    }

    async fn force_cancel_run(&self, pipeline_id: &str, _number: i64) -> PluginResult<()> {
        self.record(format!("force_cancel:{pipeline_id}"));
        Ok(())
    }

    async fn review_run(
        &self, pipeline_id: &str, _number: i64, _decision: ReviewDecision,
    ) -> PluginResult<()> {
        self.record(format!("review:{pipeline_id}"));
        Ok(())
    }

    async fn rerun_failed_jobs(&self, pipeline_id: &str, _number: i64) -> PluginResult<()> {
        self.record(format!("rerun_failed:{pipeline_id}"));
        Ok(())
    }

    async fn rerun_run(&self, pipeline_id: &str, _number: i64) -> PluginResult<()> {
        self.record(format!("rerun:{pipeline_id}"));
        Ok(())
    }

    async fn enable_pipeline(&self, pipeline_id: &str) -> PluginResult<()> {
        self.record(format!("enable:{pipeline_id}"));
        Ok(())
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        self.record(format!("parameters:{workflow_id}"));
        Ok(Vec::new())
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        self.record(format!("nodes:{}:{}", run.pipeline_id, run.run_id));
        Ok(Vec::new())
    }
}

fn adapter() -> (PluginAdapter, Calls) {
    let calls = Calls::default();
    let plugin = Recorder {
        metadata: PluginMetadata::new(
            "Recorder",
            ConfigSchema::new(),
            table_schema(default_pipeline_runs_table(), default_pipelines_table()),
            PluginCapabilities::default(),
        ),
        calls: calls.clone(),
    };
    (
        PluginAdapter::new(Box::new(plugin), TYPE.into(), PROVIDER, None),
        calls,
    )
}

fn seen(calls: &Calls) -> Vec<String> {
    calls.lock().unwrap().clone()
}

#[tokio::test]
async fn pipelines_leave_the_adapter_with_composed_ids_and_owner() {
    let (adapter, _) = adapter();

    let pipelines = adapter.fetch_pipelines().await.unwrap();

    assert_eq!(pipelines[0].id, GLOBAL);
    assert_eq!(pipelines[0].provider_id, PROVIDER);
    assert_eq!(pipelines[0].provider_type, TYPE);
}

#[tokio::test]
async fn runs_leave_the_adapter_with_composed_pipeline_and_run_ids() {
    let (adapter, calls) = adapter();

    let history = adapter.fetch_run_history(GLOBAL, 5).await.unwrap();
    let details = adapter.fetch_run_details(GLOBAL, 9).await.unwrap();
    let recent = adapter
        .fetch_recent_runs(&[GLOBAL.to_string()], 5)
        .await
        .unwrap();

    assert_eq!(
        seen(&calls),
        [
            format!("history:{LOCAL}"),
            format!("details:{LOCAL}"),
            format!("recent:{LOCAL}"),
        ]
    );
    for run in [&history[0], &details, &recent[GLOBAL][0]] {
        assert_eq!(run.pipeline_id, GLOBAL);
        assert_eq!(run.id, "recorder__3__run-9");
    }
}

#[tokio::test]
async fn every_action_hands_the_plugin_the_local_id() {
    let (adapter, calls) = adapter();

    adapter.cancel_run(GLOBAL, 1).await.unwrap();
    adapter.force_cancel_run(GLOBAL, 1).await.unwrap();
    adapter
        .review_run(GLOBAL, 1, ReviewDecision::Approve)
        .await
        .unwrap();
    adapter.rerun_failed_jobs(GLOBAL, 1).await.unwrap();
    adapter.rerun_run(GLOBAL, 1).await.unwrap();
    adapter.enable_pipeline(GLOBAL).await.unwrap();
    adapter.get_workflow_parameters(GLOBAL).await.unwrap();
    let triggered = adapter
        .trigger_pipeline(TriggerParams {
            workflow_id: GLOBAL.into(),
            inputs: None,
        })
        .await
        .unwrap();

    assert_eq!(
        seen(&calls),
        [
            "cancel",
            "force_cancel",
            "review",
            "rerun_failed",
            "rerun",
            "enable",
            "parameters",
            "trigger"
        ]
        .map(|call| format!("{call}:{LOCAL}"))
    );
    assert_eq!(triggered.run_id.as_deref(), Some("recorder__3__run-10"));
    assert_eq!(triggered.run_number, Some(10));
}

#[tokio::test]
async fn ids_of_another_type_or_provider_are_rejected_before_the_plugin_runs() {
    let (adapter, calls) = adapter();

    for foreign in [
        "other__3__owner/repo/ci.yml",
        "recorder__4__owner/repo/ci.yml",
        "recorder__3__",
        LOCAL,
    ] {
        assert!(matches!(
            adapter.fetch_run_history(foreign, 1).await,
            Err(DomainError::InvalidConfig(_))
        ));
        assert!(matches!(
            adapter.cancel_run(foreign, 1).await,
            Err(DomainError::InvalidConfig(_))
        ));
        assert!(matches!(
            adapter.get_workflow_parameters(foreign).await,
            Err(DomainError::InvalidConfig(_))
        ));
        assert!(matches!(
            adapter.fetch_recent_runs(&[foreign.to_string()], 1).await,
            Err(DomainError::InvalidConfig(_))
        ));
    }
    assert!(seen(&calls).is_empty());
}

#[tokio::test]
async fn log_calls_see_local_pipeline_and_run_ids() {
    let (adapter, calls) = adapter();
    let mut run = adapter.fetch_run_details(GLOBAL, 9).await.unwrap();

    adapter.fetch_log_nodes(&run).await.unwrap();
    run.id = "legacy-run-9".into();
    adapter.fetch_log_nodes(&run).await.unwrap();

    assert_eq!(
        seen(&calls)[1..],
        [
            format!("nodes:{LOCAL}:run-9"),
            format!("nodes:{LOCAL}:legacy-run-9"),
        ]
    );

    run.pipeline_id = "other__3__x".into();
    assert!(matches!(
        adapter.fetch_log_nodes(&run).await,
        Err(DomainError::InvalidConfig(_))
    ));
}

#[test]
fn global_and_local_ids_round_trip() {
    let (adapter, _) = adapter();

    assert_eq!(adapter.global_id(LOCAL), GLOBAL);
    assert_eq!(adapter.local_id(GLOBAL).unwrap(), LOCAL);
    assert!(adapter.local_id(LOCAL).is_err());
}
