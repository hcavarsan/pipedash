use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    WorkflowParameter,
    WorkflowParameterType,
};
use serde::de::{
    Deserializer,
    MapAccess,
    Visitor,
};
use serde::Deserialize;
use serde_json::{
    Map,
    Value,
};

pub(crate) const BRANCH: &str = "branch";
pub(crate) const DEFINITION: &str = "pipeline";
const PARAMETER_PREFIX: &str = "parameters.";

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    parameters: Option<Declared>,
}

struct Declared(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Declared {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedMap;

        impl<'de> Visitor<'de> for OrderedMap {
            type Value = Declared;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of pipeline parameters")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Declared, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Declared(entries))
            }
        }

        deserializer.deserialize_map(OrderedMap)
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn parameter_type(declared: &Value) -> Option<WorkflowParameterType> {
    let default = declared.get("default");
    Some(match declared.get("type").and_then(Value::as_str)? {
        "string" => WorkflowParameterType::String {
            default: default.map(text),
        },
        "boolean" => WorkflowParameterType::Boolean {
            default: default.and_then(Value::as_bool).unwrap_or(false),
        },
        "integer" => WorkflowParameterType::Number {
            default: default.and_then(Value::as_f64),
        },
        "enum" => WorkflowParameterType::Choice {
            options: declared
                .get("enum")
                .and_then(Value::as_array)
                .map(|options| options.iter().map(text).collect())
                .unwrap_or_default(),
            default: default.map(text),
        },
        _ => return None,
    })
}

pub(crate) fn pipeline_parameters(config_source: &str) -> PluginResult<Vec<WorkflowParameter>> {
    let file: ConfigFile = serde_saphyr::from_str(config_source)
        .map_err(|e| PluginError::ApiError(format!("Failed to parse the CircleCI config: {e}")))?;
    Ok(file
        .parameters
        .map(|declared| declared.0)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, declared)| {
            Some(WorkflowParameter {
                param_type: parameter_type(&declared)?,
                required: declared.get("default").is_none(),
                description: declared
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                label: Some(name.clone()),
                name: format!("{PARAMETER_PREFIX}{name}"),
            })
        })
        .collect())
}

pub(crate) struct TriggerInputs {
    pub branch: Option<String>,
    pub definition: Option<String>,
    pub parameters: Map<String, Value>,
}

pub(crate) fn trigger_inputs(inputs: Option<Value>) -> TriggerInputs {
    let mut values = match inputs {
        Some(Value::Object(values)) => values,
        _ => Map::new(),
    };
    let mut take = |key: &str| match values.remove(key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    };
    let branch = take(BRANCH);
    let definition = take(DEFINITION);
    let parameters = values
        .into_iter()
        .filter_map(|(key, value)| {
            let name = key.strip_prefix(PARAMETER_PREFIX)?.to_string();
            let value = match value {
                Value::Number(number) => match number.as_f64() {
                    Some(float) if float.fract() == 0.0 && float.abs() < 9.0e15 => {
                        Value::from(float as i64)
                    }
                    _ => Value::Number(number),
                },
                Value::Null => return None,
                other => other,
            };
            Some((name, value))
        })
        .collect();
    TriggerInputs {
        branch,
        definition,
        parameters,
    }
}

#[cfg(test)]
mod tests;
