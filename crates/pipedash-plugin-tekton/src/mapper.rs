use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineStatus,
    WorkflowParameter,
    WorkflowParameterType,
};

use crate::types::{
    self,
    TektonPipeline,
    TektonPipelineRun,
};

pub(crate) mod correlation;
pub(crate) mod logs;
pub(crate) mod runs;

pub(crate) fn map_status(conditions: &[types::Condition]) -> PipelineStatus {
    if let Some(succeeded_condition) = conditions.iter().find(|c| c.type_ == "Succeeded") {
        match succeeded_condition.status.as_str() {
            "True" => PipelineStatus::Success,
            "False" => match succeeded_condition.reason.as_str() {
                "Cancelled" | "PipelineRunCancelled" | "TaskRunCancelled" => {
                    PipelineStatus::Cancelled
                }
                _ => PipelineStatus::Failed,
            },
            "Unknown" => match succeeded_condition.reason.as_str() {
                "PipelineRunPending" | "Pending" => PipelineStatus::Pending,
                _ => PipelineStatus::Running,
            },
            _ => PipelineStatus::Pending,
        }
    } else {
        PipelineStatus::Pending
    }
}

pub(crate) fn pipeline_metadata(
    namespace: &str, pipeline_name: &str,
) -> HashMap<String, serde_json::Value> {
    HashMap::from([
        ("namespace".to_string(), serde_json::json!(namespace)),
        (
            "pipeline_name".to_string(),
            serde_json::json!(pipeline_name),
        ),
    ])
}

pub(crate) fn map_pipeline(
    pipeline: &TektonPipeline, latest_run: Option<&TektonPipelineRun>,
) -> Pipeline {
    let namespace = &pipeline.metadata.namespace;
    let pipeline_name = &pipeline.metadata.name;

    let status = latest_run
        .map(|run| map_status(&run.status.conditions))
        .unwrap_or(PipelineStatus::Pending);

    let last_run = latest_run.and_then(|run| types::parse_timestamp(&run.status.start_time));

    let branch = latest_run.and_then(|run| {
        run.metadata
            .labels
            .get("tekton.dev/gitBranch")
            .or_else(|| run.metadata.labels.get("git-branch"))
            .cloned()
    });

    let metadata = pipeline_metadata(namespace, pipeline_name);

    Pipeline {
        correlation: latest_run
            .map(correlation::pipeline_correlation)
            .unwrap_or_default(),
        id: format!("{namespace}__{pipeline_name}"),
        name: pipeline_name.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository: format!("{}/{}", namespace, pipeline_name),
        branch,
        workflow_file: None,
        resource_state: None,
        metadata,
    }
}

pub(crate) fn run_number(metadata: &types::ObjectMeta) -> i64 {
    let created = types::parse_timestamp(&metadata.creation_timestamp)
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    let uid = metadata
        .uid
        .as_deref()
        .and_then(|uid| u32::from_str_radix(uid.get(..8)?, 16).ok())
        .unwrap_or(0);
    created * 1_000_000 + i64::from(uid % 1_000_000)
}

pub(crate) fn map_available_pipeline(pipeline: &TektonPipeline) -> AvailablePipeline {
    let namespace = &pipeline.metadata.namespace;
    let pipeline_name = &pipeline.metadata.name;

    let description = pipeline
        .metadata
        .annotations
        .get("description")
        .cloned()
        .or_else(|| Some(format!("{}/{}", namespace, pipeline_name)));

    AvailablePipeline {
        id: format!("{}__{}", namespace, pipeline_name),
        name: pipeline_name.clone(),
        description,
        organization: Some(namespace.clone()),
        repository: Some(pipeline_name.clone()),
    }
}

pub(crate) fn map_workflow_parameters(pipeline: &TektonPipeline) -> Vec<WorkflowParameter> {
    pipeline
        .spec
        .params
        .iter()
        .map(|param| {
            let param_type = match param.param_type.as_deref() {
                Some("string") | None => WorkflowParameterType::String {
                    default: param
                        .default
                        .as_ref()
                        .and_then(|v| v.as_str().map(|s| s.to_string())),
                },
                Some("array") => WorkflowParameterType::String {
                    default: param.default.as_ref().map(|v| v.to_string()),
                },
                _ => WorkflowParameterType::String { default: None },
            };

            WorkflowParameter {
                name: param.name.clone(),
                label: Some(param.name.clone()),
                description: param.description.clone(),
                param_type,
                required: param.default.is_none(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
