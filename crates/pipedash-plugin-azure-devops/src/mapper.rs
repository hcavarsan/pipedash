use std::collections::{
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::utils::duration_seconds;
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};
use reqwest::Url;
use serde_json::{
    json,
    Map,
    Value,
};

use crate::{
    config,
    types,
};

pub(crate) const BRANCH_INPUT: &str = "source-branch";
const BRANCH_REF_PREFIX: &str = "refs/heads/";
const REF_PREFIX: &str = "refs/";

pub(crate) fn map_status(status: Option<&str>, result: Option<&str>) -> PipelineStatus {
    match (status, result) {
        (Some("completed"), Some("succeeded")) => PipelineStatus::Success,
        (Some("completed"), Some("canceled" | "abandoned")) => PipelineStatus::Cancelled,
        (Some("completed"), Some("skipped")) => PipelineStatus::Skipped,
        (Some("completed"), Some("partiallySucceeded" | "succeededWithIssues")) => {
            PipelineStatus::Failed
        }
        (Some("completed"), _) => PipelineStatus::Failed,
        (Some("inProgress" | "cancelling"), _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

fn map_build_status(build: &types::Build) -> PipelineStatus {
    map_status(build.status.as_deref(), build.result.as_deref())
}

fn timestamp(value: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    value.filter(|t| *t > DateTime::UNIX_EPOCH)
}

pub(crate) fn short_branch(source_branch: &str) -> &str {
    source_branch
        .strip_prefix(BRANCH_REF_PREFIX)
        .unwrap_or(source_branch)
}

fn branch_of(build: &types::Build) -> Option<String> {
    build
        .source_branch
        .as_deref()
        .filter(|branch| !branch.is_empty())
        .map(|branch| short_branch(branch).to_string())
}

pub(crate) fn map_available_pipeline(
    project: &types::Project, organization: &str,
) -> AvailablePipeline {
    AvailablePipeline {
        id: project.name.clone(),
        name: project.name.clone(),
        description: project.description.clone().filter(|d| !d.is_empty()),
        organization: Some(organization.to_string()),
        repository: None,
    }
}

pub(crate) fn map_pipeline(
    organization: &str, project: &str, definition: &types::Definition,
) -> Pipeline {
    let latest_build = definition.latest_build.as_ref();

    Pipeline {
        id: config::build_pipeline_id(project, definition.id),
        name: definition.name.clone(),
        status: latest_build
            .map(map_build_status)
            .unwrap_or(PipelineStatus::Pending),
        last_run: latest_build
            .and_then(|build| timestamp(build.start_time).or_else(|| timestamp(build.queue_time))),
        last_updated: Utc::now(),
        repository: format!("{organization}/{project}"),
        branch: latest_build.and_then(branch_of),
        workflow_file: None,
        resource_state: None,
        metadata: HashMap::new(),
    }
}

fn run_inputs(build: &types::Build, branch: Option<String>) -> Option<Value> {
    let mut inputs = Map::new();
    if let Some(branch) = branch {
        inputs.insert(BRANCH_INPUT.to_string(), Value::String(branch));
    }
    if let Some(Ok(Value::Object(variables))) = build
        .parameters
        .as_deref()
        .map(serde_json::from_str::<Value>)
    {
        inputs.extend(variables);
    }
    (!inputs.is_empty()).then_some(Value::Object(inputs))
}

pub(crate) fn map_run(
    build: &types::Build, pipeline_id: &str, project: &str, server_url: &Url,
) -> PipelineRun {
    let started_at = timestamp(build.start_time);
    let concluded_at = timestamp(build.finish_time);
    let branch = branch_of(build);

    PipelineRun {
        id: format!("azure-devops-run-{}", build.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: build.id,
        status: map_build_status(build),
        started_at: started_at
            .or_else(|| timestamp(build.queue_time))
            .unwrap_or_else(Utc::now),
        concluded_at,
        duration_seconds: duration_seconds(started_at, concluded_at),
        logs_url: build
            .web_link()
            .map(str::to_string)
            .unwrap_or_else(|| config::build_web_url(server_url, project, build.id)),
        commit_sha: build.source_version.clone().filter(|sha| !sha.is_empty()),
        commit_message: None,
        branch: branch.clone(),
        actor: build
            .requested_for
            .as_ref()
            .and_then(|identity| identity.display_name.clone()),
        inputs: run_inputs(build, branch),
        metadata: build
            .build_number
            .iter()
            .map(|number| ("build_number".to_string(), json!(number)))
            .collect(),
    }
}

fn record_status(record: &types::TimelineRecord) -> PipelineStatus {
    map_status(record.state.as_deref(), record.result.as_deref())
}

fn job_log(status: PipelineStatus, has_log: bool) -> Option<LogAvailability> {
    match (status, has_log) {
        (PipelineStatus::Pending, _) | (PipelineStatus::Running, false) => {
            Some(LogAvailability::Pending)
        }
        (PipelineStatus::Running, true) => Some(LogAvailability::Partial),
        (_, true) => Some(LogAvailability::Ready),
        (_, false) => None,
    }
}

fn log_node(
    record: &types::TimelineRecord, parent_id: Option<&str>, kind: LogNodeKind,
    log: Option<LogAvailability>,
) -> LogNode {
    LogNode {
        id: record.id.clone(),
        parent_id: parent_id.map(str::to_string),
        kind,
        name: record.name.clone().unwrap_or_default(),
        status: record_status(record),
        started_at: timestamp(record.start_time),
        finished_at: timestamp(record.finish_time),
        log,
        web_url: None,
    }
}

type Children<'a> = HashMap<Option<&'a str>, Vec<&'a types::TimelineRecord>>;

fn collect_log_nodes<'a>(
    children: &Children<'a>, parent: Option<&'a str>, stage: Option<&'a str>,
    nodes: &mut Vec<LogNode>,
) {
    for record in children.get(&parent).into_iter().flatten() {
        match record.kind.as_deref() {
            Some("Stage") => {
                nodes.push(log_node(record, None, LogNodeKind::Stage, None));
                collect_log_nodes(children, Some(&record.id), Some(&record.id), nodes);
            }
            Some("Phase") => collect_log_nodes(children, Some(&record.id), stage, nodes),
            Some("Job") => {
                let log = job_log(record_status(record), record.log.is_some());
                nodes.push(log_node(record, stage, LogNodeKind::Job, log));
                for task in children
                    .get(&Some(record.id.as_str()))
                    .into_iter()
                    .flatten()
                    .filter(|task| task.kind.as_deref() == Some("Task"))
                {
                    nodes.push(log_node(task, Some(&record.id), LogNodeKind::Step, None));
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn timeline_to_log_nodes(timeline: &types::Timeline) -> Vec<LogNode> {
    let mut children = Children::new();
    for record in &timeline.records {
        children
            .entry(record.parent_id.as_deref())
            .or_default()
            .push(record);
    }
    for siblings in children.values_mut() {
        siblings.sort_by_key(|record| record.order.unwrap_or(i64::MAX));
    }
    let mut nodes = Vec::with_capacity(timeline.records.len());
    collect_log_nodes(&children, None, None, &mut nodes);
    nodes
}

pub(crate) fn job_log_ref(timeline: &types::Timeline, record_id: &str) -> Option<(i64, i64)> {
    timeline
        .records
        .iter()
        .find(|record| record.id == record_id && record.kind.as_deref() == Some("Job"))
        .and_then(|record| {
            record
                .log
                .as_ref()
                .map(|log| (log.id, record.attempt.unwrap_or(0)))
        })
}

pub(crate) fn source_branch(branch: &str) -> String {
    if branch.starts_with(REF_PREFIX) {
        branch.to_string()
    } else {
        format!("{BRANCH_REF_PREFIX}{branch}")
    }
}

pub(crate) fn trigger_body(
    definition_id: i64, inputs: Option<Value>, overridable: &HashSet<String>,
) -> Value {
    let mut branch = None;
    let mut variables = Map::new();
    if let Some(Value::Object(inputs)) = inputs {
        for (name, value) in inputs {
            let text = match value {
                Value::String(text) => text,
                Value::Bool(flag) => flag.to_string(),
                Value::Number(number) => number.to_string(),
                _ => continue,
            };
            if text.trim().is_empty() {
                continue;
            }
            if name == BRANCH_INPUT {
                branch = Some(text);
            } else if overridable.contains(&name.to_lowercase()) {
                variables.insert(name, Value::String(text));
            }
        }
    }

    let mut body = json!({ "definition": { "id": definition_id } });
    if let Some(branch) = branch {
        body["sourceBranch"] = Value::String(source_branch(branch.trim()));
    }
    if !variables.is_empty() {
        body["parameters"] = Value::String(Value::Object(variables).to_string());
    }
    body
}

#[cfg(test)]
mod tests;
