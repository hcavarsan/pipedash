use std::collections::BTreeMap;

use pipedash_plugin_api::{
    ActionAvailability,
    ActionCopy,
    DisplayField,
    WorkflowParameter,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    Map,
    Value,
};

use super::PipelineService;
use crate::application::services::presenter::{
    pipeline_facts,
    run_facts,
};
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDraft {
    pub parameters: Vec<WorkflowParameter>,
    pub values: Map<String, Value>,
    pub fields: Vec<DisplayField>,
    pub errors: BTreeMap<String, String>,
    pub availability: ActionAvailability,
    pub copy: ActionCopy,
}

impl PipelineService {
    pub async fn prepare_trigger(
        &self, pipeline_id: &str, rerun_of: Option<i64>, inputs: Option<Map<String, Value>>,
    ) -> DomainResult<TriggerDraft> {
        let pipeline = self.presented_pipeline(pipeline_id).await?;
        let plugin = self
            .provider_service
            .plugin_registry()
            .get(&pipeline.provider_type)
            .ok_or_else(|| DomainError::InvalidProviderType(pipeline.provider_type.clone()))?;
        let parameters = self
            .provider_service
            .get_workflow_parameters(pipeline_id)
            .await?;
        let previous = match rerun_of {
            Some(run_number) => Some(self.fetch_run_details(pipeline_id, run_number).await?),
            None => None,
        };

        let values = inputs.unwrap_or_else(|| {
            let facts = previous.as_ref().map(run_facts);
            plugin.trigger_values(&pipeline_facts(&pipeline), facts.as_ref(), &parameters)
        });
        let presentation = &plugin.metadata().presentation;
        let copy = if rerun_of.is_some() {
            presentation.rerun.clone()
        } else {
            presentation.trigger.clone()
        };

        Ok(TriggerDraft {
            errors: validate_params(&parameters, &values),
            fields: draft_fields(&pipeline, previous.as_ref(), &plugin.metadata().name),
            availability: pipeline.actions.trigger.clone(),
            parameters,
            values,
            copy,
        })
    }
}

fn is_missing(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.trim().is_empty(),
        Some(_) => false,
    }
}

fn validate_params(
    parameters: &[WorkflowParameter], values: &Map<String, Value>,
) -> BTreeMap<String, String> {
    parameters
        .iter()
        .filter(|parameter| parameter.required && is_missing(values.get(&parameter.name)))
        .map(|parameter| {
            let label = parameter
                .label
                .as_deref()
                .filter(|label| !label.is_empty())
                .unwrap_or(&parameter.name);
            (parameter.name.clone(), format!("{label} is required"))
        })
        .collect()
}

fn text(value: &Value) -> &str {
    value.as_str().map_or("", str::trim)
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn run_environment(run: &PipelineRun) -> &str {
    run.inputs
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|inputs| {
            inputs.iter().find(|(key, _)| {
                let tail = key.rsplit('.').next().unwrap_or_default();
                tail.eq_ignore_ascii_case("env") || tail.eq_ignore_ascii_case("environment")
            })
        })
        .map_or("", |(_, value)| text(value))
}

fn replayed_inputs(run: &PipelineRun) -> impl Iterator<Item = DisplayField> + '_ {
    run.inputs
        .as_ref()
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(key, value)| {
            let blank = value.is_null() || value.as_str() == Some("");
            let branch_echo = matches!(key.as_str(), "ref" | "branch")
                && value.as_str().is_some()
                && value.as_str() == run.branch.as_deref();
            !blank && !branch_echo
        })
        .map(|(key, value)| DisplayField {
            label: key.clone(),
            value: display_value(value),
        })
}

fn commit_summary(run: &PipelineRun) -> String {
    let sha: String = run
        .commit_sha
        .as_deref()
        .map_or_else(String::new, |sha| sha.chars().take(7).collect());
    let message = run
        .commit_message
        .as_deref()
        .and_then(|message| message.lines().next())
        .map_or("", str::trim);
    [sha.as_str(), message]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn draft_fields(
    pipeline: &Pipeline, previous: Option<&PipelineRun>, provider_name: &str,
) -> Vec<DisplayField> {
    let mut fields = vec![
        ("Pipeline", pipeline.name.clone()),
        ("Repository", pipeline.repository.clone()),
    ];
    match previous {
        Some(run) => fields.extend([
            ("Provider", provider_name.to_string()),
            ("Run", format!("#{}", run.run_number)),
            ("Branch", run.branch.clone().unwrap_or_default()),
            ("Commit", commit_summary(run)),
            ("Environment", run_environment(run).to_string()),
        ]),
        None => fields.push(("Branch", pipeline.branch.clone().unwrap_or_default())),
    }

    fields
        .into_iter()
        .map(|(label, value)| DisplayField {
            label: label.to_string(),
            value,
        })
        .chain(pipeline.actions.target.clone())
        .chain(previous.into_iter().flat_map(replayed_inputs))
        .filter(|field| !field.value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests;
