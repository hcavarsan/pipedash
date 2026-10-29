use std::collections::HashMap;

use chrono::Utc;
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    WorkflowParameter,
    WorkflowParameterType,
};

use crate::types::{
    self,
    StepState,
    TektonPipeline,
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

pub(crate) fn listed_taskruns(run: &TektonPipelineRun) -> Vec<(&str, &str)> {
    let children: Vec<_> = run
        .status
        .child_references
        .iter()
        .filter(|child| child.kind == "TaskRun")
        .map(|child| (child.name.as_str(), child.pipeline_task_name.as_str()))
        .collect();
    if !children.is_empty() {
        return children;
    }

    let mut legacy: Vec<_> = run
        .status
        .task_runs
        .iter()
        .map(|(name, task_run)| (name.as_str(), task_run.pipeline_task_name.as_str()))
        .collect();
    legacy.sort_unstable();
    legacy
}

fn log_availability(status: PipelineStatus) -> LogAvailability {
    match status {
        PipelineStatus::Running => LogAvailability::Partial,
        PipelineStatus::Pending => LogAvailability::Pending,
        _ => LogAvailability::Ready,
    }
}

fn step_status(step: &StepState) -> PipelineStatus {
    match (&step.terminated, &step.running) {
        (Some(_), _) if step.termination_reason.as_deref() == Some("Skipped") => {
            PipelineStatus::Skipped
        }
        (Some(terminated), _) if terminated.exit_code == 0 => PipelineStatus::Success,
        (Some(_), _) => PipelineStatus::Failed,
        (None, Some(_)) => PipelineStatus::Running,
        (None, None) => PipelineStatus::Pending,
    }
}

fn step_to_log_node(task_run: &str, step: &StepState) -> LogNode {
    LogNode {
        id: format!("{task_run}:{}", step.name),
        parent_id: Some(task_run.to_string()),
        kind: LogNodeKind::Step,
        name: step.name.clone(),
        status: step_status(step),
        started_at: step.started_at(),
        finished_at: step.finished_at(),
        log: None,
        web_url: None,
    }
}

pub(crate) fn taskruns_to_log_nodes(
    run: &TektonPipelineRun, task_runs: &[TektonTaskRun],
) -> Vec<LogNode> {
    let mut nodes = Vec::new();
    for (name, pipeline_task) in listed_taskruns(run) {
        let Some(task_run) = task_runs
            .iter()
            .find(|task_run| task_run.metadata.name == name)
        else {
            continue;
        };
        let status = map_status(&task_run.status.conditions);

        nodes.push(LogNode {
            id: name.to_string(),
            parent_id: None,
            kind: LogNodeKind::Job,
            name: if pipeline_task.is_empty() {
                name.to_string()
            } else {
                pipeline_task.to_string()
            },
            status,
            started_at: types::parse_timestamp(&task_run.status.start_time),
            finished_at: types::parse_timestamp(&task_run.status.completion_time),
            log: Some(log_availability(status)),
            web_url: None,
        });
        nodes.extend(
            task_run
                .status
                .steps
                .iter()
                .map(|step| step_to_log_node(name, step)),
        );
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn succeeded(status: &str, reason: &str) -> Vec<types::Condition> {
        let json = format!(r#"[{{"type":"Succeeded","status":"{status}","reason":"{reason}"}}]"#);

        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn maps_each_succeeded_condition_to_a_status() {
        let cases = [
            ("Unknown", "Started", PipelineStatus::Running),
            ("Unknown", "Running", PipelineStatus::Running),
            ("Unknown", "PipelineRunStopping", PipelineStatus::Running),
            (
                "Unknown",
                "CancelledRunningFinally",
                PipelineStatus::Running,
            ),
            ("Unknown", "StoppedRunningFinally", PipelineStatus::Running),
            (
                "Unknown",
                "PipelineRunTimeoutRunningFinally",
                PipelineStatus::Running,
            ),
            ("Unknown", "PipelineRunPending", PipelineStatus::Pending),
            ("True", "Succeeded", PipelineStatus::Success),
            ("True", "Completed", PipelineStatus::Success),
            ("False", "Failed", PipelineStatus::Failed),
            ("False", "PipelineRunTimeout", PipelineStatus::Failed),
            ("False", "PipelineValidationFailed", PipelineStatus::Failed),
            ("False", "CouldntGetPipeline", PipelineStatus::Failed),
            ("False", "Cancelled", PipelineStatus::Cancelled),
            ("False", "PipelineRunCancelled", PipelineStatus::Cancelled),
            ("Unknown", "Pending", PipelineStatus::Pending),
            ("Unknown", "TaskRunCancelled", PipelineStatus::Running),
            ("False", "TaskRunCancelled", PipelineStatus::Cancelled),
            ("False", "TaskRunTimeout", PipelineStatus::Failed),
            ("False", "TaskRunImagePullFailed", PipelineStatus::Failed),
        ];

        for (status, reason, expected) in cases {
            assert_eq!(
                map_status(&succeeded(status, reason)),
                expected,
                "{status} {reason}"
            );
        }
    }

    #[test]
    fn maps_a_run_without_a_succeeded_condition_to_pending() {
        let conditions: Vec<types::Condition> =
            serde_json::from_str(r#"[{"type":"Ready","status":"True"}]"#).unwrap();

        assert_eq!(map_status(&conditions), PipelineStatus::Pending);
        assert_eq!(map_status(&[]), PipelineStatus::Pending);
    }

    fn pipeline_run(status_json: &str) -> TektonPipelineRun {
        serde_json::from_str(&format!(
            r#"{{"apiVersion":"tekton.dev/v1","kind":"PipelineRun","metadata":{{"name":"build-1","namespace":"ci"}},"spec":{{}},"status":{status_json}}}"#
        ))
        .unwrap()
    }

    fn task_run(name: &str, status_json: &str) -> TektonTaskRun {
        serde_json::from_str(&format!(
            r#"{{"metadata":{{"name":"{name}","namespace":"ci"}},"status":{status_json}}}"#
        ))
        .unwrap()
    }

    const TWO_CHILDREN: &str = r#"{"childReferences":[
        {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-clone","pipelineTaskName":"clone"},
        {"apiVersion":"tekton.dev/v1beta1","kind":"CustomRun","name":"build-1-approval","pipelineTaskName":"approval"},
        {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-test","pipelineTaskName":"test"}
    ]}"#;

    #[test]
    fn listed_taskruns_come_from_child_references_then_from_legacy_task_runs() {
        let current = pipeline_run(TWO_CHILDREN);
        assert_eq!(
            listed_taskruns(&current),
            [("build-1-clone", "clone"), ("build-1-test", "test")]
        );

        let legacy = pipeline_run(
            r#"{"taskRuns":{
                "build-1-test":{"pipelineTaskName":"test"},
                "build-1-clone":{"pipelineTaskName":"clone"}
            }}"#,
        );
        assert_eq!(
            listed_taskruns(&legacy),
            [("build-1-clone", "clone"), ("build-1-test", "test")]
        );

        assert!(listed_taskruns(&pipeline_run("{}")).is_empty());
    }

    #[test]
    fn every_taskrun_condition_maps_to_a_job_status_and_log_availability() {
        let run = pipeline_run(TWO_CHILDREN);
        let cases = [
            (
                "True",
                "Succeeded",
                PipelineStatus::Success,
                LogAvailability::Ready,
            ),
            (
                "False",
                "Failed",
                PipelineStatus::Failed,
                LogAvailability::Ready,
            ),
            (
                "False",
                "TaskRunTimeout",
                PipelineStatus::Failed,
                LogAvailability::Ready,
            ),
            (
                "False",
                "TaskRunCancelled",
                PipelineStatus::Cancelled,
                LogAvailability::Ready,
            ),
            (
                "Unknown",
                "Running",
                PipelineStatus::Running,
                LogAvailability::Partial,
            ),
            (
                "Unknown",
                "Started",
                PipelineStatus::Running,
                LogAvailability::Partial,
            ),
            (
                "Unknown",
                "Pending",
                PipelineStatus::Pending,
                LogAvailability::Pending,
            ),
        ];
        for (status, reason, expected, log) in cases {
            let json = format!(
                r#"{{"conditions":[{{"type":"Succeeded","status":"{status}","reason":"{reason}"}}]}}"#
            );
            let nodes = taskruns_to_log_nodes(&run, &[task_run("build-1-clone", &json)]);
            assert_eq!(nodes.len(), 1, "{status} {reason}");
            assert_eq!(nodes[0].status, expected, "{status} {reason}");
            assert_eq!(nodes[0].log, Some(log), "{status} {reason}");
        }

        let nodes = taskruns_to_log_nodes(&run, &[task_run("build-1-clone", "{}")]);
        assert_eq!(nodes[0].status, PipelineStatus::Pending);
        assert_eq!(nodes[0].log, Some(LogAvailability::Pending));
    }

    #[test]
    fn job_nodes_follow_the_child_references_and_skip_unlisted_or_missing_taskruns() {
        let run = pipeline_run(TWO_CHILDREN);
        let task_runs = [
            task_run("build-1-test", r#"{"startTime":"2026-10-28T10:02:00Z"}"#),
            task_run("build-1-stray", "{}"),
            task_run(
                "build-1-clone",
                r#"{"startTime":"2026-10-28T10:00:00Z","completionTime":"2026-10-28T10:01:30Z"}"#,
            ),
        ];

        let nodes = taskruns_to_log_nodes(&run, &task_runs);

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.name.as_str(),
                    node.kind,
                    node.parent_id.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("build-1-clone", "clone", LogNodeKind::Job, None),
                ("build-1-test", "test", LogNodeKind::Job, None),
            ]
        );
        assert_eq!(
            nodes[0].started_at,
            types::parse_timestamp(&Some("2026-10-28T10:00:00Z".to_string()))
        );
        assert_eq!(
            nodes[0].finished_at,
            types::parse_timestamp(&Some("2026-10-28T10:01:30Z".to_string()))
        );
        assert_eq!(nodes[1].finished_at, None);
    }

    #[test]
    fn steps_become_child_nodes_without_a_log_of_their_own() {
        let run = pipeline_run(TWO_CHILDREN);
        let steps = r#"{"steps":[
            {"name":"fetch","container":"step-fetch","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:00:01Z","finishedAt":"2026-10-28T10:00:05Z"},"terminationReason":"Completed"},
            {"name":"compile","container":"step-compile","terminated":{"exitCode":2,"startedAt":"2026-10-28T10:00:05Z","finishedAt":"2026-10-28T10:00:09Z"},"terminationReason":"Error"},
            {"name":"package","container":"step-package","terminated":{"exitCode":1,"startedAt":"2026-10-28T10:00:09Z","finishedAt":"2026-10-28T10:00:09Z"},"terminationReason":"Skipped"},
            {"name":"upload","container":"step-upload","running":{"startedAt":"2026-10-28T10:00:10Z"}},
            {"name":"notify","container":"step-notify","waiting":{"reason":"PodInitializing"}}
        ]}"#;

        let nodes = taskruns_to_log_nodes(&run, &[task_run("build-1-clone", steps)]);

        let summary: Vec<_> = nodes[1..]
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.parent_id.as_deref(),
                    node.kind,
                    node.name.as_str(),
                    node.status,
                    node.log,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "build-1-clone:fetch",
                    Some("build-1-clone"),
                    LogNodeKind::Step,
                    "fetch",
                    PipelineStatus::Success,
                    None
                ),
                (
                    "build-1-clone:compile",
                    Some("build-1-clone"),
                    LogNodeKind::Step,
                    "compile",
                    PipelineStatus::Failed,
                    None
                ),
                (
                    "build-1-clone:package",
                    Some("build-1-clone"),
                    LogNodeKind::Step,
                    "package",
                    PipelineStatus::Skipped,
                    None
                ),
                (
                    "build-1-clone:upload",
                    Some("build-1-clone"),
                    LogNodeKind::Step,
                    "upload",
                    PipelineStatus::Running,
                    None
                ),
                (
                    "build-1-clone:notify",
                    Some("build-1-clone"),
                    LogNodeKind::Step,
                    "notify",
                    PipelineStatus::Pending,
                    None
                ),
            ]
        );
        let stamp = |text: &str| types::parse_timestamp(&Some(text.to_string()));
        assert_eq!(nodes[1].started_at, stamp("2026-10-28T10:00:01Z"));
        assert_eq!(nodes[1].finished_at, stamp("2026-10-28T10:00:05Z"));
        assert_eq!(nodes[4].started_at, stamp("2026-10-28T10:00:10Z"));
        assert_eq!(nodes[4].finished_at, None);
        assert_eq!(nodes[5].started_at, None);
    }
}
