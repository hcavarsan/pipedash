use std::collections::HashMap;

use async_trait::async_trait;

use crate::schema::TableSchema;
use crate::*;

pub(crate) struct Stub {
    metadata: PluginMetadata,
    initialized: bool,
}

pub(crate) fn manifest(provider_type: &'static str) -> PluginManifest {
    PluginManifest {
        package_name: provider_type,
        version: "1.2.3",
        description: "Stub provider",
        authors: "Stub Team",
        icon_svg: "<svg/>",
    }
}

impl Stub {
    pub(crate) fn new(capabilities: PluginCapabilities) -> Self {
        Self {
            metadata: PluginMetadata::new(
                "Stub",
                ConfigSchema::new(),
                TableSchema::new(),
                capabilities,
            ),
            initialized: false,
        }
    }

    pub(crate) fn with_metadata(metadata: PluginMetadata) -> Self {
        Self {
            metadata,
            initialized: false,
        }
    }
}

#[async_trait]
impl Plugin for Stub {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, _provider_id: i64, _config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        self.initialized = true;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        Ok(self.initialized)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        Ok(Vec::new())
    }

    async fn fetch_run_history(
        &self, _pipeline_id: &str, _limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        Ok(Vec::new())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, _run_number: i64,
    ) -> PluginResult<PipelineRun> {
        Err(PluginError::PipelineNotFound(pipeline_id.to_string()))
    }

    async fn trigger_pipeline(&self, _params: TriggerParams) -> PluginResult<TriggerResult> {
        Ok(TriggerResult::default())
    }
}
