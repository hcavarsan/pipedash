use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    PluginError,
    PluginResult,
    WorkflowParameter,
    WorkflowParameterType,
};
use serde::de::{
    Deserializer,
    IgnoredAny,
    MapAccess,
    Visitor,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    config,
    types,
};

pub(crate) fn map_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    match (status, conclusion) {
        ("completed", Some("success")) => PipelineStatus::Success,
        ("completed", Some("cancelled")) => PipelineStatus::Cancelled,
        ("completed", Some("skipped")) => PipelineStatus::Skipped,
        ("completed", _) => PipelineStatus::Failed,
        ("in_progress", _) | ("queued", _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

fn timestamp(value: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    value.filter(|t| *t > DateTime::UNIX_EPOCH)
}

pub(crate) fn map_available_pipeline(repo: &types::Repository) -> AvailablePipeline {
    AvailablePipeline {
        id: repo.full_name.clone(),
        name: repo.full_name.clone(),
        description: Some(repo.description.clone()).filter(|d| !d.is_empty()),
        organization: Some(repo.owner.login.clone()),
        repository: Some(repo.name.clone()),
    }
}

pub(crate) fn map_pipeline(
    provider_id: i64, owner: &str, repo: &str, workflow: &types::Workflow,
    latest_run: Option<&types::Run>,
) -> Pipeline {
    Pipeline {
        id: config::build_pipeline_id(provider_id, owner, repo, &workflow.id),
        provider_id,
        provider_type: "gitea".to_string(),
        name: workflow.name.clone(),
        status: latest_run
            .map(|run| map_status(&run.status, run.conclusion.as_deref()))
            .unwrap_or(PipelineStatus::Pending),
        last_run: latest_run.and_then(|run| timestamp(run.started_at)),
        last_updated: Utc::now(),
        repository: format!("{owner}/{repo}"),
        branch: latest_run.map(|run| run.head_branch.clone()),
        workflow_file: Some(workflow.path.clone()),
        metadata: HashMap::new(),
    }
}

pub(crate) fn map_run(run: &types::Run, pipeline_id: &str, owner: &str) -> PipelineRun {
    let started_at = timestamp(run.started_at);
    let concluded_at = timestamp(run.completed_at);

    PipelineRun {
        id: format!("gitea-run-{}", run.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: run.run_number,
        status: map_status(&run.status, run.conclusion.as_deref()),
        started_at: started_at.unwrap_or_else(Utc::now),
        concluded_at,
        duration_seconds: started_at
            .zip(concluded_at)
            .map(|(started, concluded)| (concluded - started).num_seconds()),
        logs_url: run.html_url.clone(),
        commit_sha: Some(run.head_sha.clone()),
        commit_message: Some(run.display_title.clone()),
        branch: Some(run.head_branch.clone()),
        actor: run.trigger_actor.as_ref().map(|actor| actor.login.clone()),
        inputs: Some(serde_json::json!({ "ref": run.head_branch })),
        metadata: HashMap::from([
            ("event".to_string(), serde_json::json!(run.event)),
            ("run_id".to_string(), serde_json::json!(run.id)),
            ("owner".to_string(), serde_json::json!(owner)),
        ]),
    }
}

#[derive(Deserialize)]
struct WorkflowFile {
    #[serde(default)]
    on: Option<Triggers>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Triggers {
    Map {
        #[serde(default)]
        workflow_dispatch: Option<Dispatch>,
    },
    Other(IgnoredAny),
}

#[derive(Deserialize)]
struct Dispatch {
    #[serde(default)]
    inputs: Option<Inputs>,
}

struct Inputs(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Inputs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedMap;

        impl<'de> Visitor<'de> for OrderedMap {
            type Value = Inputs;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of workflow inputs")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Inputs, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Inputs(entries))
            }
        }

        deserializer.deserialize_map(OrderedMap)
    }
}

const INPUT_PREFIX: &str = "inputs.";

pub(crate) fn dispatch_inputs(workflow_source: &str) -> PluginResult<Vec<WorkflowParameter>> {
    let file: WorkflowFile = serde_saphyr::from_str(workflow_source)
        .map_err(|e| PluginError::ApiError(format!("Failed to parse workflow file: {e}")))?;
    let inputs = match file.on {
        Some(Triggers::Map {
            workflow_dispatch: Some(Dispatch {
                inputs: Some(inputs),
            }),
        }) => inputs.0,
        _ => Vec::new(),
    };

    Ok(inputs
        .into_iter()
        .map(|(name, input)| WorkflowParameter {
            name: format!("{INPUT_PREFIX}{name}"),
            label: Some(name),
            description: input
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            param_type: input_type(&input),
            required: input
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect())
}

pub(crate) fn dispatch_body(inputs: Option<Value>) -> Value {
    let mut values = inputs
        .and_then(|inputs| inputs.as_object().cloned())
        .unwrap_or_default();
    let ref_value = values
        .remove("ref")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "main".to_string());
    let inputs: serde_json::Map<String, Value> = values
        .into_iter()
        .map(|(key, value)| {
            (
                key.strip_prefix(INPUT_PREFIX).unwrap_or(&key).to_string(),
                value,
            )
        })
        .collect();

    let mut body = serde_json::json!({ "ref": ref_value });
    if !inputs.is_empty() {
        body["inputs"] = Value::Object(inputs);
    }
    body
}

fn input_type(input: &Value) -> WorkflowParameterType {
    let default = input.get("default");
    let text = |value: &Value| match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    match input
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("string")
    {
        "boolean" => WorkflowParameterType::Boolean {
            default: default
                .is_some_and(|d| d.as_bool() == Some(true) || d.as_str() == Some("true")),
        },
        "number" => WorkflowParameterType::Number {
            default: default.and_then(|d| d.as_f64().or_else(|| d.as_str()?.parse().ok())),
        },
        "choice" => WorkflowParameterType::Choice {
            options: input
                .get("options")
                .and_then(Value::as_array)
                .map(|options| options.iter().map(text).collect())
                .unwrap_or_default(),
            default: default.map(text),
        },
        _ => WorkflowParameterType::String {
            default: default.map(text),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_inputs_are_read_from_the_workflow_file() {
        let source = "name: Release\non:\n  push:\n  workflow_dispatch:\n    inputs:\n      version:\n        description: Release version\n        required: true\n      dry_run:\n        type: boolean\n        default: true\n      level:\n        type: choice\n        options: [debug, info]\n        default: info\n      retries:\n        type: number\n        default: 3\njobs: {}\n";
        let params = dispatch_inputs(source).unwrap();
        let summary: Vec<_> = params
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.required,
                    serde_json::to_value(&p.param_type).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "inputs.version",
                    true,
                    serde_json::json!({"type": "string", "default": null})
                ),
                (
                    "inputs.dry_run",
                    false,
                    serde_json::json!({"type": "boolean", "default": true})
                ),
                (
                    "inputs.level",
                    false,
                    serde_json::json!({"type": "choice", "options": ["debug", "info"], "default": "info"})
                ),
                (
                    "inputs.retries",
                    false,
                    serde_json::json!({"type": "number", "default": 3.0})
                ),
            ]
        );
    }

    #[test]
    fn dispatch_body_keeps_a_workflow_input_named_ref_separate_from_the_branch() {
        let body = dispatch_body(Some(serde_json::json!({
            "ref": "release",
            "inputs.ref": "abc123",
            "inputs.greeting": "hi"
        })));
        assert_eq!(
            body,
            serde_json::json!({"ref": "release", "inputs": {"ref": "abc123", "greeting": "hi"}})
        );
        assert_eq!(dispatch_body(None), serde_json::json!({"ref": "main"}));
    }

    #[test]
    fn workflows_without_dispatch_inputs_have_no_parameters() {
        assert!(dispatch_inputs("on: push\njobs: {}\n").unwrap().is_empty());
        assert!(dispatch_inputs("on: [push, workflow_dispatch]\n")
            .unwrap()
            .is_empty());
        assert!(dispatch_inputs("on:\n  workflow_dispatch:\n")
            .unwrap()
            .is_empty());
    }
    #[test]
    fn status_mapping_follows_github_style_status_and_conclusion() {
        let cases = [
            ("completed", Some("success"), PipelineStatus::Success),
            ("completed", Some("failure"), PipelineStatus::Failed),
            ("completed", Some("cancelled"), PipelineStatus::Cancelled),
            ("completed", Some("skipped"), PipelineStatus::Skipped),
            ("in_progress", None, PipelineStatus::Running),
            ("queued", None, PipelineStatus::Running),
            ("waiting", None, PipelineStatus::Pending),
            ("unknown", None, PipelineStatus::Pending),
        ];
        for (status, conclusion, expected) in cases {
            assert_eq!(map_status(status, conclusion), expected, "{status}");
        }
    }

    #[test]
    fn epoch_zero_timestamps_are_treated_as_unset() {
        let run: types::Run = serde_json::from_value(serde_json::json!({
            "id": 2, "run_number": 2, "status": "queued", "path": "ci.yaml@refs/heads/main",
            "html_url": "http://g/o/r/actions/runs/2",
            "started_at": "1970-01-01T00:00:00Z", "completed_at": "1970-01-01T00:00:00Z"
        }))
        .unwrap();
        let mapped = map_run(&run, "gitea__1__o/r/ci.yaml", "o");
        assert_eq!(mapped.concluded_at, None);
        assert_eq!(mapped.duration_seconds, None);
        assert_eq!(run.workflow_file(), "ci.yaml");
    }
}
