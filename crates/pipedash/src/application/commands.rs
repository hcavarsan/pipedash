use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::Plugin as PluginTrait;
use serde::{
    Deserialize,
    Serialize,
};
use tauri::State;

use crate::application::services::{
    PipelineService,
    ProviderService,
};
use crate::application::RefreshManager;
use crate::domain::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    ProviderConfig,
    ProviderSummary,
    TriggerParams,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl From<crate::domain::DomainError> for ErrorResponse {
    fn from(err: crate::domain::DomainError) -> Self {
        ErrorResponse {
            error: err.to_string(),
        }
    }
}

pub struct AppState {
    pub provider_service: Arc<ProviderService>,
    pub pipeline_service: Arc<PipelineService>,
    pub refresh_manager: Arc<RefreshManager>,
}

#[tauri::command]
pub async fn add_provider(
    state: State<'_, AppState>, config: ProviderConfig,
) -> Result<i64, ErrorResponse> {
    let id = state
        .provider_service
        .add_provider(config)
        .await
        .map_err(ErrorResponse::from)?;

    let _ = state.pipeline_service.fetch_pipelines(Some(id)).await;

    Ok(id)
}

#[tauri::command]
pub async fn list_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderSummary>, ErrorResponse> {
    state
        .provider_service
        .list_providers()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_provider(
    state: State<'_, AppState>, id: i64,
) -> Result<ProviderConfig, ErrorResponse> {
    state
        .provider_service
        .get_provider_config(id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_provider(
    state: State<'_, AppState>, id: i64, config: ProviderConfig,
) -> Result<(), ErrorResponse> {
    state
        .provider_service
        .update_provider(id, config)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_provider_refresh_interval(
    state: State<'_, AppState>, id: i64, refresh_interval: i64,
) -> Result<(), ErrorResponse> {
    if refresh_interval < 5 {
        return Err(ErrorResponse {
            error: "Refresh interval must be at least 5 seconds".to_string(),
        });
    }

    state
        .provider_service
        .update_provider_refresh_interval(id, refresh_interval)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn remove_provider(state: State<'_, AppState>, id: i64) -> Result<(), ErrorResponse> {
    state
        .provider_service
        .remove_provider(id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn fetch_pipelines(
    state: State<'_, AppState>, provider_id: Option<i64>,
) -> Result<Vec<Pipeline>, ErrorResponse> {
    state
        .pipeline_service
        .fetch_pipelines(provider_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_cached_pipelines(
    state: State<'_, AppState>, provider_id: Option<i64>,
) -> Result<Vec<Pipeline>, ErrorResponse> {
    state
        .pipeline_service
        .get_cached_pipelines(provider_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn fetch_run_history(
    state: State<'_, AppState>, pipeline_id: String, page: Option<usize>, page_size: Option<usize>,
) -> Result<crate::domain::PaginatedRunHistory, ErrorResponse> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);

    eprintln!(
        "[COMMAND] fetch_run_history: pipeline={}, page={}, page_size={}",
        pipeline_id, page, page_size
    );

    state
        .pipeline_service
        .fetch_run_history_paginated(&pipeline_id, page, page_size)
        .await
        .map_err(ErrorResponse::from)
}

#[tauri::command]
pub async fn trigger_pipeline(
    state: State<'_, AppState>, params: TriggerParams,
) -> Result<String, ErrorResponse> {
    state
        .pipeline_service
        .trigger_pipeline(params)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_all(state: State<'_, AppState>) -> Result<(), ErrorResponse> {
    // Clear all run history caches on full refresh
    state.pipeline_service.clear_all_run_history_caches().await;

    state
        .pipeline_service
        .refresh_all()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_run_history_cache(
    state: State<'_, AppState>, pipeline_id: String,
) -> Result<(), ErrorResponse> {
    state
        .pipeline_service
        .clear_run_history_cache(&pipeline_id)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn set_refresh_mode(
    state: State<'_, AppState>, mode: String,
) -> Result<(), ErrorResponse> {
    let refresh_mode = match mode.as_str() {
        "active" => crate::application::refresh_manager::RefreshMode::Active,
        "idle" => crate::application::refresh_manager::RefreshMode::Idle,
        _ => {
            return Err(ErrorResponse {
                error: format!("Invalid refresh mode: {mode}"),
            })
        }
    };

    state.refresh_manager.set_mode(refresh_mode).await;
    Ok(())
}

#[tauri::command]
pub async fn get_refresh_mode(state: State<'_, AppState>) -> Result<String, ErrorResponse> {
    let mode = state.refresh_manager.get_mode().await;
    Ok(match mode {
        crate::application::refresh_manager::RefreshMode::Active => "active".to_string(),
        crate::application::refresh_manager::RefreshMode::Idle => "idle".to_string(),
    })
}

#[tauri::command]
pub async fn get_workflow_run_details(
    state: State<'_, AppState>, pipeline_id: String, run_number: i64,
) -> Result<PipelineRun, ErrorResponse> {
    let run = state
        .pipeline_service
        .fetch_run_details(&pipeline_id, run_number)
        .await
        .map_err(ErrorResponse::from)?;

    eprintln!(
        "[COMMAND] Returning run details with inputs: {:?}",
        run.inputs.is_some()
    );
    if let Some(ref inputs) = run.inputs {
        eprintln!(
            "[COMMAND] Inputs JSON: {}",
            serde_json::to_string(inputs).unwrap_or_default()
        );
    }

    Ok(run)
}

#[tauri::command]
pub async fn cancel_pipeline_run(
    state: State<'_, AppState>, pipeline_id: String, run_number: i64,
) -> Result<(), ErrorResponse> {
    eprintln!("[COMMAND] Cancelling run #{run_number} for pipeline {pipeline_id}");

    state
        .pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await
        .map_err(ErrorResponse::from)?;

    eprintln!("[COMMAND] Run cancellation requested successfully");
    Ok(())
}

#[tauri::command]
pub async fn get_available_plugins(
) -> Result<Vec<pipedash_plugin_api::PluginMetadata>, ErrorResponse> {
    let mut registry = pipedash_plugin_api::PluginRegistry::new();

    pipedash_plugin_github::register(&mut registry);
    pipedash_plugin_buildkite::register(&mut registry);
    pipedash_plugin_jenkins::register(&mut registry);

    // Collect metadata from all registered plugins
    let mut metadata_list = Vec::new();
    for provider_type in registry.provider_types() {
        if let Some(plugin) = registry.get(&provider_type) {
            metadata_list.push(plugin.metadata().clone());
        }
    }

    Ok(metadata_list)
}

#[tauri::command]
pub async fn preview_provider_pipelines(
    provider_type: String, token: String, config: HashMap<String, String>,
) -> Result<Vec<AvailablePipeline>, ErrorResponse> {
    // Create a new plugin instance based on provider type
    let mut plugin: Box<dyn PluginTrait> = if provider_type == "github" {
        Box::new(pipedash_plugin_github::GitHubPlugin::new())
    } else if provider_type == "buildkite" {
        Box::new(pipedash_plugin_buildkite::BuildkitePlugin::new())
    } else if provider_type == "jenkins" {
        Box::new(pipedash_plugin_jenkins::JenkinsPlugin::new())
    } else {
        return Err(ErrorResponse {
            error: format!("Unknown provider type: {provider_type}"),
        });
    };

    let mut plugin_config = config.clone();
    plugin_config.insert("token".to_string(), token);

    plugin
        .initialize(0, plugin_config)
        .map_err(|e| ErrorResponse {
            error: format!("Failed to initialize plugin: {e}"),
        })?;

    plugin
        .validate_credentials()
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to validate credentials: {e}"),
        })?;

    let plugin_pipelines = plugin
        .fetch_available_pipelines()
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch available pipelines: {e}"),
        })?;

    let pipelines = plugin_pipelines
        .into_iter()
        .map(|p| AvailablePipeline {
            id: p.id,
            name: p.name,
            description: p.description,
            organization: p.organization,
            repository: p.repository,
        })
        .collect();

    Ok(pipelines)
}

#[tauri::command]
pub async fn get_workflow_parameters(
    workflow_id: String, state: tauri::State<'_, AppState>,
) -> Result<Vec<pipedash_plugin_api::WorkflowParameter>, ErrorResponse> {
    let parts: Vec<&str> = workflow_id.split("__").collect();
    if parts.len() < 2 {
        return Err(ErrorResponse {
            error: format!(
                "Invalid workflow ID format '{}'. Expected format: 'provider__id__...'",
                workflow_id
            ),
        });
    }

    let provider_id: i64 = parts[1].parse().map_err(|_| ErrorResponse {
        error: format!(
            "Invalid provider ID '{}' in workflow ID. Expected numeric ID.",
            parts[1]
        ),
    })?;

    state
        .provider_service
        .get_workflow_parameters(provider_id, &workflow_id)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch workflow parameters: {e}"),
        })
}
