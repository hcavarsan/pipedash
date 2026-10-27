mod log_dialect;

pub use log_dialect::ActionsLogDialect;
use pipedash_plugin_api::{
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

pub fn dispatch_inputs(workflow_source: &str) -> PluginResult<Vec<WorkflowParameter>> {
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

pub fn dispatch_body(inputs: Option<Value>) -> Value {
    let mut values = match inputs {
        Some(Value::Object(values)) => values,
        _ => serde_json::Map::new(),
    };
    let ref_value = match values.remove("ref") {
        Some(Value::String(value)) => value,
        _ => "main".to_string(),
    };
    let inputs: serde_json::Map<String, Value> = values
        .into_iter()
        .map(|(mut key, value)| {
            if key.starts_with(INPUT_PREFIX) {
                key.replace_range(..INPUT_PREFIX.len(), "");
            }
            (key, value)
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
}
