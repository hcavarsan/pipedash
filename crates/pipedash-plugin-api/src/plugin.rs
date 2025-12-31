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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub provider_type: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub icon: Option<String>,
    pub config_schema: ConfigSchema,
    pub table_schema: TableSchema,
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub required_permissions: Vec<Permission>,
    #[serde(default)]
    pub features: Vec<Feature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginCapabilities {
    pub pipelines: bool,
    pub pipeline_runs: bool,
    pub trigger: bool,
    pub agents: bool,
    pub artifacts: bool,
    pub queues: bool,
    pub custom_tables: bool,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()>;

    async fn validate_credentials(&self) -> PluginResult<bool>;

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

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>>;

    async fn fetch_pipelines_paginated(
        &self, page: usize, page_size: usize,
    ) -> PluginResult<crate::types::PaginatedResponse<Pipeline>> {
        let all_pipelines = self.fetch_pipelines().await?;
        let total_count = all_pipelines.len();
        let start = (page - 1) * page_size;
        let end = start + page_size;

        let pipelines = if start < total_count {
            all_pipelines[start..end.min(total_count)].to_vec()
        } else {
            Vec::new()
        };

        Ok(crate::types::PaginatedResponse::new(
            pipelines,
            page,
            page_size,
            total_count,
        ))
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>>;

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun>;

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String>;

    async fn cancel_run(&self, _pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Run cancellation not supported by this provider".to_string(),
        ))
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(Vec::new())
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Err(crate::error::PluginError::NotSupported(
            "Agent monitoring not supported by this provider".to_string(),
        ))
    }

    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Err(crate::error::PluginError::NotSupported(
            "Artifact fetching not supported by this provider".to_string(),
        ))
    }

    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Err(crate::error::PluginError::NotSupported(
            "Queue monitoring not supported by this provider".to_string(),
        ))
    }

    fn get_migrations(&self) -> Vec<String> {
        Vec::new()
    }

    fn provider_type(&self) -> &str {
        &self.metadata().provider_type
    }

    async fn get_field_options(
        &self, _field_key: &str, _config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        Ok(Vec::new())
    }

    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        let required_permissions = &self.metadata().required_permissions;
        let permissions = required_permissions
            .iter()
            .map(|p| PermissionCheck {
                permission: p.clone(),
                granted: true,
            })
            .collect();

        Ok(PermissionStatus {
            permissions,
            all_granted: true,
            checked_at: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
        })
    }

    fn get_feature_availability(&self, status: &PermissionStatus) -> Vec<FeatureAvailability> {
        let features = &self.metadata().features;
        let granted_perms: std::collections::HashSet<String> = status
            .permissions
            .iter()
            .filter(|p| p.granted)
            .map(|p| p.permission.name.clone())
            .collect();

        features
            .iter()
            .map(|feature| {
                let missing: Vec<String> = feature
                    .required_permissions
                    .iter()
                    .filter(|p| !granted_perms.contains(*p))
                    .cloned()
                    .collect();

                FeatureAvailability {
                    feature: feature.clone(),
                    available: missing.is_empty(),
                    missing_permissions: missing,
                }
            })
            .collect()
    }
}
