//! Data mapping utilities for Jenkins

use std::collections::HashSet;

use chrono::Utc;
use pipedash_plugin_api::{
    PipelineRun,
    PipelineStatus,
    WorkflowParameter,
    WorkflowParameterType,
};

use crate::types;

/// Maps Jenkins build result to PipelineStatus
pub(crate) fn map_jenkins_result(result: Option<&str>) -> PipelineStatus {
    match result {
        Some("SUCCESS") => PipelineStatus::Success,
        Some("FAILURE") => PipelineStatus::Failed,
        Some("UNSTABLE") => PipelineStatus::Failed,
        Some("ABORTED") => PipelineStatus::Cancelled,
        Some("NOT_BUILT") => PipelineStatus::Skipped,
        None => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

/// Converts Jenkins Build to PipelineRun
pub(crate) fn build_to_pipeline_run(
    build: types::Build, pipeline_id: &str, server_url: &str, encoded_path: &str,
) -> PipelineRun {
    let status = if build.building {
        PipelineStatus::Running
    } else {
        map_jenkins_result(build.result.as_deref())
    };

    let started_at = chrono::DateTime::from_timestamp_millis(build.timestamp)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let concluded_at = if build.duration > 0 {
        Some(
            chrono::DateTime::from_timestamp_millis(build.timestamp + build.duration)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        )
    } else {
        None
    };

    let duration_seconds = if build.duration > 0 {
        Some(build.duration / 1000)
    } else {
        None
    };

    let mut commit_sha = String::from("unknown");
    let mut branch = String::from("unknown");
    let mut actor = String::from("unknown");
    let mut inputs: Option<serde_json::Value> = None;

    eprintln!(
        "[MAPPER] Processing build #{} with {} actions",
        build.number,
        build.actions.len()
    );

    for (idx, action) in build.actions.iter().enumerate() {
        eprintln!(
            "[MAPPER] Action #{}: _class={:?}, has {} parameters, has {} causes",
            idx,
            action._class,
            action.parameters.len(),
            action.causes.len()
        );

        if let Some(ref revision) = action.last_built_revision {
            if let Some(first_branch) = revision.branch.first() {
                commit_sha = first_branch.sha1.clone();
                branch = first_branch.name.clone();
            }
        }
        if let Some(first_cause) = action.causes.first() {
            if let Some(ref user) = first_cause.user_name {
                actor = user.clone();
            }
        }
        // Extract parameters from ParametersAction
        if !action.parameters.is_empty() {
            eprintln!(
                "[MAPPER] Found {} parameters in action",
                action.parameters.len()
            );
            let mut params_map = serde_json::Map::new();
            for param in &action.parameters {
                eprintln!("[MAPPER] Parameter: {} = {:?}", param.name, param.value);
                params_map.insert(param.name.clone(), param.value.clone());
            }
            let params_count = params_map.len();
            inputs = Some(serde_json::Value::Object(params_map));
            eprintln!("[MAPPER] Built inputs map with {params_count} entries");
        }
    }

    eprintln!("[MAPPER] Final inputs: {inputs:?}");

    PipelineRun {
        id: format!("jenkins-build-{}", build.number),
        pipeline_id: pipeline_id.to_string(),
        run_number: build.number,
        status,
        started_at,
        concluded_at,
        duration_seconds,
        logs_url: build
            .url
            .clone()
            .unwrap_or_else(|| format!("{}/job/{}/{}", server_url, encoded_path, build.number)),
        commit_sha,
        commit_message: None,
        branch,
        actor,
        inputs,
    }
}

/// Converts Jenkins parameter definitions to WorkflowParameters
pub(crate) fn parameter_definitions_to_workflow_parameters(
    param_definitions: Vec<types::ParameterDefinition>,
) -> Vec<WorkflowParameter> {
    let mut parameters = Vec::with_capacity(param_definitions.len());
    let mut seen_param_names = HashSet::with_capacity(param_definitions.len());

    for param_def in param_definitions {
        if !seen_param_names.insert(param_def.name.clone()) {
            continue;
        }

        let param_type = if let Some(class) = &param_def._class {
            if class.contains("BooleanParameterDefinition") || class.contains("BooleanParameter") {
                let default_value = param_def
                    .default_parameter_value
                    .and_then(|dpv| dpv.value)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                WorkflowParameterType::Boolean {
                    default: default_value,
                }
            } else if (class.contains("ChoiceParameterDefinition")
                || class.contains("ChoiceParameter")
                || class.contains("CascadeChoiceParameter")
                || class.contains("DynamicReferenceParameter"))
                && !param_def.choices.is_empty()
            {
                let mut choice_seen = HashSet::with_capacity(param_def.choices.len());
                let mut cleaned_choices = Vec::with_capacity(param_def.choices.len());
                let mut selected_default: Option<String> = None;

                for choice in param_def.choices {
                    if let Some(clean_choice) = choice.strip_suffix(":selected") {
                        let clean = clean_choice.to_string();
                        if choice_seen.insert(clean.clone()) {
                            if selected_default.is_none() {
                                selected_default = Some(clean.clone());
                            }
                            cleaned_choices.push(clean);
                        }
                    } else if choice_seen.insert(choice.clone()) {
                        cleaned_choices.push(choice);
                    }
                }

                let default_value = selected_default
                    .or_else(|| {
                        param_def
                            .default_parameter_value
                            .and_then(|dpv| dpv.value)
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                    .or_else(|| cleaned_choices.first().cloned());

                WorkflowParameterType::Choice {
                    options: cleaned_choices,
                    default: default_value,
                }
            } else {
                let default_value = param_def
                    .default_parameter_value
                    .and_then(|dpv| dpv.value)
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                WorkflowParameterType::String {
                    default: default_value,
                }
            }
        } else {
            let default_value = param_def
                .default_parameter_value
                .and_then(|dpv| dpv.value)
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            WorkflowParameterType::String {
                default: default_value,
            }
        };

        parameters.push(WorkflowParameter {
            name: param_def.name.clone(),
            label: Some(param_def.name),
            description: param_def.description,
            param_type,
            required: false,
        });
    }

    parameters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_jenkins_result() {
        assert_eq!(map_jenkins_result(Some("SUCCESS")), PipelineStatus::Success);
        assert_eq!(map_jenkins_result(Some("FAILURE")), PipelineStatus::Failed);
        assert_eq!(
            map_jenkins_result(Some("ABORTED")),
            PipelineStatus::Cancelled
        );
        assert_eq!(map_jenkins_result(None), PipelineStatus::Running);
    }
}
