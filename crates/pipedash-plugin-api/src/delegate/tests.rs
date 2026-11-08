use std::collections::HashMap;

use crate::schema::TableSchema;
use crate::*;

fn mock_metadata() -> PluginMetadata {
    PluginMetadata::new(
        "Mock",
        ConfigSchema::new(),
        TableSchema::new(),
        PluginCapabilities::default(),
    )
}

plugin_struct!(
    MockPlugin,
    String,
    mock_metadata(); status_source: Option<StatusSource> = None,
    calls: usize = 7
);

impl MockPlugin {
    async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        Ok(Vec::new())
    }

    async fn cancel_run_impl(&self, _pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        Err(PluginError::ApiError(format!("cancel {run_number}")))
    }

    async fn fetch_run_history_impl(
        &self, _pipeline_id: &str, _limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        Ok(Vec::new())
    }

    async fn fetch_run_details_impl(
        &self, pipeline_id: &str, _run_number: i64,
    ) -> PluginResult<PipelineRun> {
        Err(PluginError::PipelineNotFound(pipeline_id.to_string()))
    }

    async fn trigger_pipeline_impl(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        Ok(TriggerResult {
            run_id: Some(params.workflow_id),
            run_number: None,
        })
    }
}

plugin_impl!(MockPlugin [
    metadata,
    status_source,
    fetch_pipelines,
    fetch_run_history,
    fetch_run_details,
    trigger_pipeline,
    cancel_run,
] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        self.client = Some("client".to_string());
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        Ok(self.client()? == "client")
    }
});

#[test]
fn struct_macro_builds_default_state_and_guards_accessors() {
    let plugin = MockPlugin::default();

    assert_eq!(plugin.calls, 7);
    assert!(plugin.config.is_empty());
    assert!(matches!(plugin.client(), Err(PluginError::Internal(_))));
}

#[tokio::test]
async fn impl_macro_keeps_custom_items_and_delegates_listed_methods() {
    let mut plugin = MockPlugin::new();
    plugin
        .initialize(3, HashMap::new(), None)
        .expect("initialize");

    assert_eq!(plugin.metadata().name, "Mock");
    assert!(plugin.status_source().is_none());
    assert!(plugin.validate_credentials().await.expect("credentials"));
    assert!(plugin
        .fetch_pipelines()
        .await
        .expect("pipelines")
        .is_empty());
    assert!(matches!(
        plugin.fetch_run_details("p", 1).await,
        Err(PluginError::PipelineNotFound(id)) if id == "p"
    ));
    assert!(matches!(
        plugin.cancel_run("p", 9).await,
        Err(PluginError::ApiError(message)) if message == "cancel 9"
    ));
}
