use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::utils::{
    duration_seconds,
    parse_timestamp,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    ResourceState,
};
use serde_json::json;

use crate::config::PipelineRef;
use crate::types::{
    CronSpec,
    Meta,
    Workflow,
};

const CREATOR_LABEL: &str = "workflows.argoproj.io/creator-preferred-username";

fn timestamp(value: &Option<String>) -> Option<DateTime<Utc>> {
    parse_timestamp(value.as_deref()?)
}

pub(crate) fn map_status(workflow: &Workflow) -> PipelineStatus {
    match workflow.status.phase.as_deref() {
        Some("Running") => PipelineStatus::Running,
        Some("Succeeded") => PipelineStatus::Success,
        Some("Failed" | "Error") if workflow.spec.shutdown.is_some() => PipelineStatus::Cancelled,
        Some("Failed" | "Error") => PipelineStatus::Failed,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn run_number(metadata: &Meta) -> i64 {
    let created = timestamp(&metadata.creation_timestamp).map_or(0, |time| time.timestamp());
    let uid = metadata
        .uid
        .as_deref()
        .and_then(|uid| u32::from_str_radix(uid.get(..8)?, 16).ok())
        .unwrap_or(0);
    created * 1_000_000 + i64::from(uid % 1_000_000)
}

pub(crate) fn newest_first(workflows: &mut [Workflow]) {
    workflows.sort_by_key(|workflow| std::cmp::Reverse(run_number(&workflow.metadata)));
}

pub(crate) fn map_run(pipeline: &PipelineRef, workflow: &Workflow) -> PipelineRun {
    let status = &workflow.status;
    let started = timestamp(&status.started_at);
    let finished = timestamp(&status.finished_at);
    let metadata = [
        ("namespace", Some(&pipeline.namespace)),
        ("workflow", Some(&workflow.metadata.name)),
        ("progress", status.progress.as_ref()),
        ("message", status.message.as_ref()),
    ]
    .into_iter()
    .filter_map(|(key, value)| Some((key.to_string(), json!(value?))))
    .collect();

    PipelineRun {
        id: format!("{}__{}", pipeline.namespace, workflow.metadata.name),
        pipeline_id: pipeline.id(),
        run_number: run_number(&workflow.metadata),
        status: map_status(workflow),
        started_at: started
            .or_else(|| timestamp(&workflow.metadata.creation_timestamp))
            .unwrap_or_else(Utc::now),
        concluded_at: finished,
        duration_seconds: duration_seconds(started, finished),
        logs_url: String::new(),
        commit_sha: None,
        commit_message: None,
        branch: None,
        actor: workflow.metadata.labels.get(CREATOR_LABEL).cloned(),
        inputs: None,
        metadata,
    }
}

fn schedule(cron: Option<&CronSpec>) -> Option<String> {
    let cron = cron?;
    let schedules: Vec<&str> = cron
        .schedule
        .iter()
        .chain(&cron.schedules)
        .map(String::as_str)
        .collect();
    (!schedules.is_empty()).then(|| schedules.join(", "))
}

pub(crate) fn map_pipeline(
    pipeline: &PipelineRef, cron: Option<&CronSpec>, latest: Option<&Workflow>,
) -> Pipeline {
    let mut metadata = HashMap::from([
        ("namespace".to_string(), json!(pipeline.namespace)),
        ("kind".to_string(), json!(pipeline.kind.label())),
    ]);
    if let Some(schedule) = schedule(cron) {
        metadata.insert("schedule".to_string(), json!(schedule));
    }

    Pipeline {
        id: pipeline.id(),
        name: pipeline.name.clone(),
        status: latest.map_or(PipelineStatus::Pending, map_status),
        last_run: latest.and_then(|run| timestamp(&run.status.started_at)),
        last_updated: Utc::now(),
        repository: format!("{}/{}", pipeline.namespace, pipeline.name),
        branch: None,
        workflow_file: None,
        resource_state: cron
            .filter(|cron| cron.suspend)
            .map(|_| ResourceState::new("Suspended", None)),
        metadata,
    }
}

pub(crate) fn map_available(pipeline: &PipelineRef) -> AvailablePipeline {
    AvailablePipeline {
        id: pipeline.id(),
        name: pipeline.name.clone(),
        description: Some(format!(
            "{} in {}",
            pipeline.kind.label(),
            pipeline.namespace
        )),
        organization: Some(pipeline.namespace.clone()),
        repository: Some(pipeline.name.clone()),
    }
}

#[cfg(test)]
mod tests;
