use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    WorkflowParameter,
    WorkflowParameterType,
};

use crate::types::{
    self,
    TektonPipeline,
    TektonPipelineRun,
};

fn parse_tasks_message(message: &str) -> Option<(usize, usize)> {
    let completed_start = message.find("Tasks Completed: ")? + "Tasks Completed: ".len();
    let completed_end = message[completed_start..].find(" ")?;
    let completed = message[completed_start..completed_start + completed_end]
        .parse::<usize>()
        .ok()?;

    let failed_start = message.find("Failed: ")? + "Failed: ".len();
    let failed_end = message[failed_start..].find(",")?;
    let failed = message[failed_start..failed_start + failed_end]
        .parse::<usize>()
        .ok()?;

    Some((completed, failed))
}

pub(crate) fn map_status(conditions: &[types::Condition]) -> PipelineStatus {
    if let Some(succeeded_condition) = conditions.iter().find(|c| c.type_ == "Succeeded") {
        match succeeded_condition.status.as_str() {
            "True" => PipelineStatus::Success,
            "False" => {
                if succeeded_condition.reason == "PipelineRunCancelled" {
                    PipelineStatus::Cancelled
                } else {
                    PipelineStatus::Failed
                }
            }
            "Unknown" => PipelineStatus::Running,
            _ => PipelineStatus::Pending,
        }
    } else {
        PipelineStatus::Pending
    }
}

pub(crate) fn map_pipeline(
    pipeline: &TektonPipeline, latest_run: Option<&TektonPipelineRun>, provider_id: i64,
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

    // Populate Tekton-specific metadata
    let mut metadata = HashMap::new();
    metadata.insert("namespace".to_string(), serde_json::json!(namespace));
    metadata.insert(
        "pipeline_name".to_string(),
        serde_json::json!(pipeline_name),
    );

    Pipeline {
        id: format!("tekton__{}__{}__{}", provider_id, namespace, pipeline_name),
        provider_id,
        provider_type: "tekton".to_string(),
        name: pipeline_name.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository: format!("{}/{}", namespace, pipeline_name),
        branch,
        workflow_file: None,
        metadata,
    }
}

pub(crate) fn map_pipeline_run(run: &TektonPipelineRun, provider_id: i64) -> PipelineRun {
    let namespace = &run.metadata.namespace;
    let pipeline_name = run
        .spec
        .pipeline_ref
        .as_ref()
        .map(|pr| pr.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let run_name = &run.metadata.name;

    let started_at = types::parse_timestamp(&run.status.start_time)
        .or_else(|| types::parse_timestamp(&run.metadata.creation_timestamp))
        .unwrap_or_else(Utc::now);

    let concluded_at = types::parse_timestamp(&run.status.completion_time);

    let duration_seconds = types::parse_timestamp(&run.status.start_time)
        .zip(types::parse_timestamp(&run.status.completion_time))
        .map(|(start, end)| (end - start).num_seconds());

    let commit_sha = run
        .metadata
        .labels
        .get("tekton.dev/gitRevision")
        .or_else(|| run.metadata.labels.get("git-revision"))
        .cloned();

    let branch = run
        .metadata
        .labels
        .get("tekton.dev/gitBranch")
        .or_else(|| run.metadata.labels.get("git-branch"))
        .cloned();

    let actor = run
        .metadata
        .annotations
        .get("tekton.dev/triggeredBy")
        .or_else(|| run.metadata.annotations.get("triggered-by"))
        .cloned()
        .or_else(|| {
            if run
                .metadata
                .labels
                .contains_key("triggers.tekton.dev/eventlistener")
            {
                Some("EventListener".to_string())
            } else if run
                .metadata
                .labels
                .contains_key("triggers.tekton.dev/trigger")
            {
                Some("Trigger".to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            if run
                .metadata
                .annotations
                .contains_key("kubectl.kubernetes.io/last-applied-configuration")
            {
                Some("kubectl".to_string())
            } else {
                run.metadata
                    .labels
                    .get("app.kubernetes.io/created-by")
                    .cloned()
            }
        });

    let logs_url = run
        .metadata
        .annotations
        .get("tekton.dev/url")
        .cloned()
        .unwrap_or_else(|| format!("/namespaces/{}/pipelineruns/{}", namespace, run_name));

    let triggers_event_id = run
        .metadata
        .labels
        .get("triggers.tekton.dev/triggers-eventid")
        .cloned();

    let run_number = types::parse_timestamp(&run.metadata.creation_timestamp)
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let inputs = if !run.spec.params.is_empty() {
        let params_map: std::collections::HashMap<String, serde_json::Value> = run
            .spec
            .params
            .iter()
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect();
        Some(serde_json::to_value(params_map).unwrap_or(serde_json::Value::Null))
    } else {
        None
    };

    let mut metadata = HashMap::new();
    metadata.insert("namespace".to_string(), serde_json::json!(namespace));
    metadata.insert(
        "pipeline_name".to_string(),
        serde_json::json!(pipeline_name),
    );
    metadata.insert("pipelinerun_name".to_string(), serde_json::json!(run_name));

    let run_identifier = triggers_event_id
        .as_ref()
        .map(|id| {
            let short_id: String = id
                .chars()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("event-{}", short_id)
        })
        .unwrap_or_else(|| run_name.clone());

    metadata.insert(
        "run_identifier".to_string(),
        serde_json::json!(run_identifier),
    );

    // Add trigger info if available
    if let Some(trigger) = &actor {
        metadata.insert("trigger".to_string(), serde_json::json!(trigger));
    }

    // Add event type from labels if available
    if let Some(event_type) = run
        .metadata
        .labels
        .get("triggers.tekton.dev/eventlistener")
        .or_else(|| run.metadata.labels.get("tekton.dev/trigger"))
    {
        metadata.insert("event_type".to_string(), serde_json::json!(event_type));
    }

    if let Some(timeout) = &run.spec.timeout {
        metadata.insert("timeout".to_string(), serde_json::json!(timeout));
    }

    // Add service account
    if let Some(service_account) = run
        .spec
        .task_run_template
        .as_ref()
        .and_then(|trt| trt.service_account_name.as_ref())
    {
        metadata.insert(
            "service_account".to_string(),
            serde_json::json!(service_account),
        );
    }

    let task_count = if !run.status.task_runs.is_empty() {
        run.status.task_runs.len()
    } else {
        run.status
            .child_references
            .iter()
            .filter(|cr| cr.kind == "TaskRun")
            .count()
    };

    if task_count > 0 {
        if !run.status.task_runs.is_empty() {
            let successful_tasks = run
                .status
                .task_runs
                .values()
                .filter(|tr| {
                    tr.status
                        .as_ref()
                        .and_then(|s| {
                            s.conditions
                                .iter()
                                .find(|c| c.type_ == "Succeeded")
                                .map(|c| c.status == "True")
                        })
                        .unwrap_or(false)
                })
                .count();

            let task_summary = format!("{}/{}", successful_tasks, task_count);
            metadata.insert("task_summary".to_string(), serde_json::json!(task_summary));
        } else if let Some(succeeded_condition) = run
            .status
            .conditions
            .iter()
            .find(|c| c.type_ == "Succeeded")
        {
            if let Some((completed, failed)) = parse_tasks_message(&succeeded_condition.message) {
                let successful = completed.saturating_sub(failed);
                let task_summary = format!("{}/{}", successful, completed);
                metadata.insert("task_summary".to_string(), serde_json::json!(task_summary));
            }
        }
    }

    if let Some(failed_condition) = run
        .status
        .conditions
        .iter()
        .find(|c| c.type_ == "Succeeded" && c.status == "False")
    {
        if !failed_condition.reason.is_empty() {
            metadata.insert(
                "failure_reason".to_string(),
                serde_json::json!(failed_condition.reason),
            );
        }
    }

    if !run.spec.workspaces.is_empty() {
        let workspace_types: Vec<String> = run
            .spec
            .workspaces
            .iter()
            .map(|ws| {
                if ws.persistent_volume_claim.is_some() {
                    "PVC".to_string()
                } else if ws.empty_dir.is_some() {
                    "EmptyDir".to_string()
                } else if ws.config_map.is_some() {
                    "ConfigMap".to_string()
                } else if ws.secret.is_some() {
                    "Secret".to_string()
                } else {
                    "Unknown".to_string()
                }
            })
            .collect();

        metadata.insert(
            "workspace_types".to_string(),
            serde_json::json!(workspace_types.join(", ")),
        );
    }

    PipelineRun {
        id: format!("tekton__{}__{}__{}", provider_id, namespace, run_name),
        pipeline_id: format!("tekton__{}__{}__{}", provider_id, namespace, pipeline_name),
        run_number,
        status: map_status(&run.status.conditions),
        started_at,
        concluded_at,
        duration_seconds,
        logs_url,
        commit_sha,
        commit_message: None,
        branch,
        actor,
        inputs,
        metadata,
    }
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
