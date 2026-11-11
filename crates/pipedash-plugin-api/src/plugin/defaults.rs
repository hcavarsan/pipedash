use serde_json::{
    Map,
    Value,
};

use crate::actions::{
    ActionAvailability,
    PipelineActionState,
    RunActionState,
};
use crate::metadata::PluginCapabilities;
use crate::presentation::{
    DisplayField,
    PipelineFacts,
    RunDisplay,
    RunFacts,
};
use crate::types::{
    PipelineStatus,
    WorkflowParameter,
    WorkflowParameterType,
};

fn flag(metadata: &std::collections::HashMap<String, Value>, key: &str) -> bool {
    metadata.get(key) == Some(&Value::Bool(true))
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Number(value)) => value.as_f64().is_some_and(|value| value != 0.0),
        Some(_) => true,
    }
}

pub fn pipeline_actions(
    capabilities: &PluginCapabilities, pipeline: &PipelineFacts<'_>,
) -> PipelineActionState {
    PipelineActionState {
        trigger: ActionAvailability::allowed(capabilities.trigger),
        cancel: ActionAvailability::allowed(capabilities.cancel),
        force_cancel: ActionAvailability::allowed(capabilities.force_cancel),
        enable: ActionAvailability::allowed(flag(pipeline.metadata, "can_enable")),
        target: namespace_target(pipeline.metadata),
        ..PipelineActionState::default()
    }
}

fn namespace_target(metadata: &std::collections::HashMap<String, Value>) -> Option<DisplayField> {
    let namespace = metadata.get("namespace").and_then(Value::as_str)?.trim();
    (!namespace.is_empty()).then(|| DisplayField {
        label: "Namespace".to_string(),
        value: namespace.to_string(),
    })
}

pub fn run_actions(capabilities: &PluginCapabilities, run: &RunFacts<'_>) -> RunActionState {
    let active = matches!(
        run.status,
        PipelineStatus::Running | PipelineStatus::Pending
    );
    let awaiting_review = flag(run.metadata, "awaiting_review");

    RunActionState {
        cancel: ActionAvailability::allowed(active && capabilities.cancel && !awaiting_review),
        force_cancel: ActionAvailability::allowed(
            run.status == PipelineStatus::Running && capabilities.force_cancel,
        ),
        rerun: ActionAvailability::allowed(!active && capabilities.trigger),
        rerun_failed: ActionAvailability::allowed(
            !active && flag(run.metadata, "can_rerun_failed_jobs"),
        ),
        review: ActionAvailability::allowed(awaiting_review),
        watch: active,
    }
}

pub fn run_display(run: &RunFacts<'_>) -> RunDisplay {
    let context = [("Branch", run.branch), ("Actor", run.actor)]
        .into_iter()
        .filter_map(|(label, value)| {
            value
                .filter(|value| !value.is_empty())
                .map(|value| DisplayField {
                    label: label.to_string(),
                    value: value.to_string(),
                })
        })
        .collect();

    RunDisplay {
        title: format!("#{}", run.run_number),
        context,
        started_label: "Started".to_string(),
    }
}

pub fn trigger_values(
    previous: Option<&RunFacts<'_>>, parameters: &[WorkflowParameter],
) -> Map<String, Value> {
    let mut inputs = previous
        .and_then(|run| run.inputs)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if let Some(branch) = previous
        .and_then(|run| run.branch)
        .filter(|branch| !branch.is_empty())
    {
        if !truthy(inputs.get("branch")) && !truthy(inputs.get("ref")) {
            inputs.insert("ref".to_string(), Value::String(branch.to_string()));
        }
    }

    let mut values = Map::new();
    for parameter in parameters {
        if let Some(replayed) = inputs.get(&parameter.name) {
            values.insert(parameter.name.clone(), replayed.clone());
        } else if let Some(default) = default_value(&parameter.param_type) {
            values.insert(parameter.name.clone(), default);
        }
    }
    values
}

fn default_value(parameter_type: &WorkflowParameterType) -> Option<Value> {
    match parameter_type {
        WorkflowParameterType::String { default }
        | WorkflowParameterType::Choice { default, .. } => default
            .as_deref()
            .filter(|default| !default.trim().is_empty())
            .map(Value::from),
        WorkflowParameterType::Boolean { default } => Some(Value::Bool(*default)),
        WorkflowParameterType::Number { default } => default.map(Value::from),
    }
}
