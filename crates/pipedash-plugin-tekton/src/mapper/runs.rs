use chrono::Utc;
use pipedash_plugin_api::utils;
use pipedash_plugin_api::{
    Observed,
    PipelineRun,
    RunEvidence,
};

use super::correlation::run_correlation;
use super::{
    map_status,
    pipeline_metadata,
    run_number,
};
use crate::types::{
    self,
    TektonPipelineRun,
    TektonTaskRun,
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

fn task_summary(run: &TektonPipelineRun) -> Option<String> {
    let status = &run.status;
    if !status.task_runs.is_empty() {
        let successful = status
            .task_runs
            .values()
            .filter(|task_run| {
                task_run.status.as_ref().is_some_and(|status| {
                    status
                        .conditions
                        .iter()
                        .find(|c| c.type_ == "Succeeded")
                        .is_some_and(|c| c.status == "True")
                })
            })
            .count();
        return Some(format!("{}/{}", successful, status.task_runs.len()));
    }

    if !status
        .child_references
        .iter()
        .any(|cr| cr.kind == "TaskRun")
    {
        return None;
    }
    let succeeded = status.conditions.iter().find(|c| c.type_ == "Succeeded")?;
    let (completed, failed) = parse_tasks_message(&succeeded.message)?;

    Some(format!(
        "{}/{}",
        completed.saturating_sub(failed),
        completed
    ))
}

pub(crate) fn map_pipeline_run(run: &TektonPipelineRun, tasks: &[TektonTaskRun]) -> PipelineRun {
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

    let duration_seconds = utils::duration_seconds(
        types::parse_timestamp(&run.status.start_time),
        types::parse_timestamp(&run.status.completion_time),
    );

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

    let run_number = run_number(&run.metadata);

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

    let mut metadata = pipeline_metadata(namespace, &pipeline_name);
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

    if let Some(trigger) = &actor {
        metadata.insert("trigger".to_string(), serde_json::json!(trigger));
    }

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

    if let Some(task_summary) = task_summary(run) {
        metadata.insert("task_summary".to_string(), serde_json::json!(task_summary));
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

    let timed = types::parse_timestamp(&run.status.start_time).is_some();
    let evidence = RunEvidence {
        timing: if timed {
            Observed::Recorded
        } else {
            Observed::Absent
        },
        ..RunEvidence::default()
    };

    PipelineRun {
        correlation: run_correlation(run, tasks),
        evidence,
        id: format!("{namespace}__{run_name}"),
        pipeline_id: format!("{namespace}__{pipeline_name}"),
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
