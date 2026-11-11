use std::collections::{
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    LogAvailability,
    LogNode,
    LogNodeKind,
    Observed,
    PipelineRun,
    PipelineStatus,
    RunEvidence,
    WorkflowParameter,
    WorkflowParameterType,
};

pub(crate) mod correlation;

use self::correlation::build_correlation;
use crate::types;

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

pub(crate) fn build_status(build: &types::Build) -> PipelineStatus {
    if build.building {
        PipelineStatus::Running
    } else {
        map_jenkins_result(build.result.as_deref())
    }
}

fn build_name(build: &types::Build, job_path: &str) -> String {
    match build
        .full_display_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => name.to_string(),
        None => format!(
            "{} #{}",
            job_path.rsplit('/').next().unwrap_or(job_path),
            build.number
        ),
    }
}

pub(crate) fn build_to_log_node(build: &types::Build, job_path: &str) -> LogNode {
    let status = build_status(build);
    let started_at =
        DateTime::from_timestamp_millis(build.timestamp).filter(|_| build.timestamp > 0);
    let finished_at = if build.building {
        None
    } else {
        started_at.and(DateTime::from_timestamp_millis(
            build.timestamp + build.duration,
        ))
    };
    let log = match status {
        PipelineStatus::Running => LogAvailability::Partial,
        PipelineStatus::Pending => LogAvailability::Pending,
        _ => LogAvailability::Ready,
    };

    LogNode {
        id: build.number.to_string(),
        parent_id: None,
        kind: LogNodeKind::Job,
        name: build_name(build, job_path),
        status,
        started_at,
        finished_at,
        log: Some(log),
        web_url: build.url.clone(),
    }
}

pub(crate) fn build_to_pipeline_run(
    build: types::Build, pipeline_id: &str, server_url: &str, encoded_path: &str,
) -> PipelineRun {
    let status = build_status(&build);

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

    let mut commit_sha: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut actor: Option<String> = None;
    let mut inputs: Option<serde_json::Value> = None;
    let mut trigger_cause: Option<String> = None;

    tracing::debug!(
        build_number = build.number,
        action_count = build.actions.len(),
        "Processing Jenkins build"
    );

    for (idx, action) in build.actions.iter().enumerate() {
        tracing::trace!(
            idx = idx,
            class = ?action._class,
            param_count = action.parameters.len(),
            cause_count = action.causes.len(),
            "Processing Jenkins action"
        );

        if let Some(ref revision) = action.last_built_revision {
            if let Some(first_branch) = revision.branch.first() {
                commit_sha = Some(first_branch.sha1.clone());
                branch = Some(first_branch.name.clone());
            }
        }
        if let Some(first_cause) = action.causes.first() {
            if let Some(ref user) = first_cause.user_name {
                actor = Some(user.clone());
            }
            if let Some(ref description) = first_cause.short_description {
                trigger_cause = Some(description.clone());
            }
        }
        if !action.parameters.is_empty() {
            tracing::debug!(
                param_count = action.parameters.len(),
                "Found Jenkins parameters in action"
            );
            let mut params_map = serde_json::Map::new();
            for param in &action.parameters {
                tracing::trace!(name = %param.name, "Jenkins parameter");
                params_map.insert(param.name.clone(), param.value.clone());
            }
            let params_count = params_map.len();
            inputs = Some(serde_json::Value::Object(params_map));
            tracing::debug!(count = params_count, "Built Jenkins inputs map");
        }
    }

    let mut metadata = HashMap::new();
    if let Some(cause) = trigger_cause {
        metadata.insert("trigger_cause".to_string(), serde_json::json!(cause));
    }

    let correlation = build_correlation(&build);
    let timed = build.timestamp > 0;
    let duration_seconds = duration_seconds.filter(|_| timed);
    let evidence = RunEvidence {
        timing: if timed {
            Observed::Recorded
        } else {
            Observed::Absent
        },
        ..RunEvidence::default()
    };

    PipelineRun {
        correlation,
        evidence,
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
        metadata,
    }
}

fn default_string(default: Option<types::DefaultValue>) -> Option<String> {
    default
        .and_then(|dpv| dpv.value)
        .and_then(|v| v.as_str().map(str::to_string))
}

fn choice_parameter_type(parameter: types::ParameterDefinition) -> WorkflowParameterType {
    let mut choice_seen = HashSet::with_capacity(parameter.choices.len());
    let mut cleaned_choices = Vec::with_capacity(parameter.choices.len());
    let mut selected_default: Option<String> = None;

    for choice in parameter.choices {
        let clean = choice
            .strip_suffix(":selected")
            .map(str::to_string)
            .unwrap_or_else(|| choice.clone());
        if !choice_seen.insert(clean.clone()) {
            continue;
        }
        if choice.ends_with(":selected") && selected_default.is_none() {
            selected_default = Some(clean.clone());
        }
        cleaned_choices.push(clean);
    }

    let default = selected_default
        .or_else(|| default_string(parameter.default_parameter_value))
        .or_else(|| cleaned_choices.first().cloned());

    WorkflowParameterType::Choice {
        options: cleaned_choices,
        default,
    }
}

fn parameter_type(parameter: types::ParameterDefinition) -> WorkflowParameterType {
    let class = parameter._class.clone().unwrap_or_default();
    let is_boolean =
        class.contains("BooleanParameterDefinition") || class.contains("BooleanParameter");
    let is_choice = class.contains("ChoiceParameterDefinition")
        || class.contains("ChoiceParameter")
        || class.contains("CascadeChoiceParameter")
        || class.contains("DynamicReferenceParameter");

    if is_boolean {
        let default = parameter
            .default_parameter_value
            .and_then(|dpv| dpv.value)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return WorkflowParameterType::Boolean { default };
    }
    if is_choice && !parameter.choices.is_empty() {
        return choice_parameter_type(parameter);
    }
    WorkflowParameterType::String {
        default: default_string(parameter.default_parameter_value),
    }
}

pub(crate) fn parameter_definitions_to_workflow_parameters(
    param_definitions: Vec<types::ParameterDefinition>,
) -> Vec<WorkflowParameter> {
    let mut parameters = Vec::with_capacity(param_definitions.len());
    let mut seen_param_names = HashSet::with_capacity(param_definitions.len());

    for param_def in param_definitions {
        if !seen_param_names.insert(param_def.name.clone()) {
            continue;
        }

        let name = param_def.name.clone();
        let description = param_def.description.clone();
        parameters.push(WorkflowParameter {
            name: name.clone(),
            label: Some(name),
            description,
            param_type: parameter_type(param_def),
            required: false,
        });
    }

    parameters
}

#[cfg(test)]
mod tests;
