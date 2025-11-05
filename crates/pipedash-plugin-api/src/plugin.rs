use std::collections::HashMap;

use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::PluginResult;
use crate::schema::{
    ConfigSchema,
    TableSchema,
};
use crate::types::*;

/// Plugin metadata - describes the plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name (e.g., "GitHub Actions")
    pub name: String,
    /// Plugin identifier (e.g., "github")
    pub provider_type: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
    /// Plugin author
    pub author: Option<String>,
    /// Plugin icon (URL or identifier)
    pub icon: Option<String>,
    /// Configuration schema for generic UI
    pub config_schema: ConfigSchema,
    /// Table schema for dynamic tables and columns
    pub table_schema: TableSchema,
    /// Plugin capabilities
    pub capabilities: PluginCapabilities,
}

/// Plugin capabilities - what features the plugin supports
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginCapabilities {
    /// Supports fetching pipelines/workflows
    pub pipelines: bool,
    /// Supports fetching pipeline runs/builds
    pub pipeline_runs: bool,
    /// Supports triggering pipelines
    pub trigger: bool,
    /// Supports build agents monitoring
    pub agents: bool,
    /// Supports build artifacts
    pub artifacts: bool,
    /// Supports build queues
    pub queues: bool,
    /// Requires custom database tables
    pub custom_tables: bool,
}

/// Main plugin trait - all providers must implement this
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Initialize plugin with configuration
    fn initialize(&mut self, provider_id: i64, config: HashMap<String, String>)
        -> PluginResult<()>;

    /// Validate credentials/configuration
    async fn validate_credentials(&self) -> PluginResult<bool>;

    /// Fetch available pipelines for selection
    async fn fetch_available_pipelines(
        &self, params: Option<crate::types::PaginationParams>,
    ) -> PluginResult<crate::types::PaginatedAvailablePipelines> {
        let _ = params;
        Ok(crate::types::PaginatedResponse::empty())
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<crate::types::Organization>> {
        Ok(Vec::new())
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>,
        params: Option<crate::types::PaginationParams>,
    ) -> PluginResult<crate::types::PaginatedAvailablePipelines> {
        let response = self.fetch_available_pipelines(params.clone()).await?;

        let mut filtered_items = response.items;

        if let Some(org_filter) = org {
            filtered_items.retain(|p| p.organization.as_ref() == Some(&org_filter));
        }

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            filtered_items.retain(|p| {
                p.name.to_lowercase().contains(&search_lower)
                    || p.id.to_lowercase().contains(&search_lower)
                    || p.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&search_lower))
            });
        }

        let total_count = filtered_items.len();
        let params = params.unwrap_or_default();

        Ok(crate::types::PaginatedResponse::new(
            filtered_items,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    /// Fetch all pipelines/workflows
    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>>;

    /// Fetch run history for a specific pipeline
    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>>;

    /// Fetch details for a specific pipeline run
    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun>;

    /// Trigger a pipeline
    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String>;

    /// Cancel a running pipeline/build
    async fn cancel_run(&self, _pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Run cancellation not supported by this provider".to_string(),
        ))
    }

    /// Fetch workflow parameters for a specific workflow/pipeline
    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(Vec::new())
    }

    /// Fetch build agents (optional, for Buildkite-like providers)
    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Err(crate::error::PluginError::NotSupported(
            "Agent monitoring not supported by this provider".to_string(),
        ))
    }

    /// Fetch build artifacts for a run (optional)
    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Err(crate::error::PluginError::NotSupported(
            "Artifact fetching not supported by this provider".to_string(),
        ))
    }

    /// Fetch build queues (optional)
    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Err(crate::error::PluginError::NotSupported(
            "Queue monitoring not supported by this provider".to_string(),
        ))
    }

    /// Get SQL migration statements for custom tables (optional)
    fn get_migrations(&self) -> Vec<String> {
        Vec::new()
    }

    /// Get the provider type string
    fn provider_type(&self) -> &str {
        &self.metadata().provider_type
    }

    async fn get_field_options(
        &self, _field_key: &str, _config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        Ok(Vec::new())
    }
}
