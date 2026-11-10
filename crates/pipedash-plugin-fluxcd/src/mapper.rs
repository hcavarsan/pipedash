use std::collections::{
    HashMap,
    HashSet,
};

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
    ResourceState,
};
use serde_json::Value;

use self::history::{
    helm_candidate,
    kustomization_candidate,
    new_run,
    seconds_major,
    Candidate,
    MINOR_BASE,
    SNAPSHOT_MINOR,
};
use self::metadata::{
    base_metadata,
    current_revision,
    display_name,
    latest_release_version,
    put,
    repository,
    source_ref,
    split_revision,
    state_metadata,
    workflow_file,
};
use self::state::{
    current_status,
    fresh,
    headline,
    status_label,
};
use crate::{
    config::Target,
    types::{
        FluxResource,
        Kind,
    },
};

pub(crate) mod history;
mod metadata;
pub(crate) mod state;

type Metadata = HashMap<String, Value>;

const RECORD_SNAPSHOT: &str = "current_snapshot";

pub(crate) fn target_of(resource: &FluxResource) -> Target {
    Target {
        kind: resource.kind,
        namespace: resource.metadata.namespace.clone(),
        name: resource.metadata.name.clone(),
    }
}

fn invalid(resource: &FluxResource, detail: &str) -> PluginError {
    PluginError::SerializationError(format!("{}: {detail}", target_of(resource)))
}

pub(crate) fn parse_go_duration(text: &str) -> Option<std::time::Duration> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }

    let mut seconds = 0f64;
    while !rest.is_empty() {
        let number_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let (number, tail) = rest.split_at(number_end);
        let unit_end = tail
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(tail.len());
        let (unit, next) = tail.split_at(unit_end);
        let scale = match unit {
            "ns" => 1e-9,
            "us" | "\u{b5}s" | "\u{3bc}s" => 1e-6,
            "ms" => 1e-3,
            "s" => 1.0,
            "m" => 60.0,
            "h" => 3600.0,
            _ => return None,
        };
        seconds += number.parse::<f64>().ok()? * scale;
        rest = next;
    }

    std::time::Duration::try_from_secs_f64(seconds).ok()
}

fn status_since(resource: &FluxResource) -> PluginResult<(DateTime<Utc>, &'static str)> {
    resource
        .status
        .conditions
        .iter()
        .filter(|condition| {
            matches!(
                condition.type_.as_str(),
                "Ready" | "Reconciling" | "Stalled"
            )
        })
        .filter_map(|condition| condition.last_transition_time)
        .max()
        .map(|time| (time, "condition.lastTransitionTime"))
        .or_else(|| {
            resource
                .metadata
                .creation_timestamp
                .map(|time| (time, "metadata.creationTimestamp"))
        })
        .ok_or_else(|| {
            invalid(
                resource,
                "has neither condition transition times nor creationTimestamp",
            )
        })
}

fn humanize(value: &str) -> String {
    let mut text = value.replace('-', " ");
    if let Some(first) = text.get_mut(..1) {
        first.make_ascii_uppercase();
    }

    text
}

fn snapshot_run(
    resource: &FluxResource, pipeline_id: &str, status: PipelineStatus,
    newest_history_number: Option<i64>,
) -> PluginResult<PipelineRun> {
    let (since, time_source) = status_since(resource)?;
    let number = match newest_history_number {
        Some(newest) => newest / MINOR_BASE * MINOR_BASE + SNAPSHOT_MINOR,
        None => {
            seconds_major(resource.metadata.creation_timestamp.unwrap_or(since)) + SNAPSHOT_MINOR
        }
    };
    let (branch, commit_sha) = current_revision(resource)
        .map(split_revision)
        .unwrap_or_default();

    let (reason, message) = headline(resource, status);
    let mut metadata = base_metadata(resource);
    metadata.extend(state_metadata(resource, reason));
    put(&mut metadata, "status_label", Some(status_label(status)));
    put(&mut metadata, "message", Some(message));
    put(&mut metadata, "record_source", Some(RECORD_SNAPSHOT));
    put(&mut metadata, "time_source", Some(time_source));
    put(&mut metadata, "run_label", Some("Current state"));
    put(&mut metadata, "snapshot", Some(true));

    let mut run = new_run(pipeline_id, status, since, metadata);
    run.id = format!("{pipeline_id}__{number}");
    run.run_number = number;
    run.branch = branch.map(str::to_string);
    run.commit_sha = commit_sha.map(str::to_string);

    Ok(run)
}

fn covers_current(resource: &FluxResource, newest: &Candidate, current: PipelineStatus) -> bool {
    if !matches!(current, PipelineStatus::Success | PipelineStatus::Failed)
        || newest.run.status != current
    {
        return false;
    }

    match resource.kind {
        Kind::Kustomization => {
            fresh(resource, "Ready").is_some_and(|ready| ready.reason == newest.reason)
        }
        Kind::HelmRelease => true,
    }
}

pub(crate) fn describes_current_state(run: &PipelineRun) -> bool {
    run.metadata.get("record_source").and_then(Value::as_str) == Some(RECORD_SNAPSHOT)
        || run.metadata.contains_key("message_source")
}

pub(crate) fn build_runs(resource: &FluxResource) -> PluginResult<Vec<PipelineRun>> {
    let pipeline_id = target_of(resource).selection_id();
    let current = current_status(resource);
    let latest_version = latest_release_version(resource);

    let mut candidates = resource
        .status
        .history
        .iter()
        .map(|entry| match resource.kind {
            Kind::Kustomization => kustomization_candidate(resource, &pipeline_id, entry),
            Kind::HelmRelease => helm_candidate(resource, &pipeline_id, entry, latest_version),
        })
        .collect::<PluginResult<Vec<_>>>()?;

    let mut seen = HashSet::new();
    if let Some(duplicate) = candidates
        .iter()
        .find(|candidate| !seen.insert(candidate.run.run_number))
    {
        return Err(invalid(
            resource,
            &format!(
                "status.history has several records with identity {}; refusing to expose ambiguous records",
                duplicate.run.run_number
            ),
        ));
    }

    let newest = candidates
        .iter()
        .enumerate()
        .max_by_key(|(_, candidate)| {
            candidate
                .run
                .concluded_at
                .unwrap_or(candidate.run.started_at)
        })
        .map(|(index, _)| index);
    let covered = newest.filter(|&index| covers_current(resource, &candidates[index], current));

    let snapshot = match covered {
        Some(index) => {
            let (reason, message) = headline(resource, current);
            let metadata = &mut candidates[index].run.metadata;
            put(metadata, "condition_reason", Some(reason));
            put(metadata, "message", Some(message));
            put(metadata, "message_source", Some("current Ready condition"));
            None
        }
        None => Some(snapshot_run(
            resource,
            &pipeline_id,
            current,
            candidates
                .iter()
                .map(|candidate| candidate.run.run_number)
                .max(),
        )?),
    };

    let mut runs: Vec<PipelineRun> = candidates
        .into_iter()
        .map(|candidate| candidate.run)
        .chain(snapshot)
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse((run.started_at.timestamp_millis(), run.run_number)));

    Ok(runs)
}

fn degraded_message(workload_issues: &[String]) -> Option<String> {
    match workload_issues.split_first()? {
        (first, []) => Some(first.clone()),
        (first, rest) => Some(format!("{first} (+{} more)", rest.len())),
    }
}

pub(crate) fn degrade_current_run(runs: &mut [PipelineRun], workload_issues: &[String]) {
    let Some(message) = degraded_message(workload_issues) else {
        return;
    };
    let current = runs
        .iter_mut()
        .filter(|run| run.status == PipelineStatus::Success && describes_current_state(run));
    for run in current {
        run.status = PipelineStatus::Degraded;
        put(
            &mut run.metadata,
            "status_label",
            Some(status_label(PipelineStatus::Degraded)),
        );
        put(&mut run.metadata, "message", Some(message.as_str()));
    }
}

pub(crate) fn map_pipeline(resource: &FluxResource, workload_issues: &[String]) -> Pipeline {
    let flux_status = current_status(resource);
    let (reason, flux_message) = headline(resource, flux_status);
    let degraded = (flux_status == PipelineStatus::Success)
        .then(|| degraded_message(workload_issues))
        .flatten();
    let (status, message) = match degraded {
        Some(message) => (PipelineStatus::Degraded, message),
        None => (flux_status, flux_message),
    };
    let (branch, _) = current_revision(resource)
        .map(split_revision)
        .unwrap_or_default();
    let mut metadata = base_metadata(resource);
    metadata.extend(state_metadata(resource, reason));

    Pipeline {
        id: target_of(resource).selection_id(),
        name: display_name(resource),
        status,
        last_run: resource
            .status
            .history
            .iter()
            .filter_map(|entry| entry.last_reconciled.or(entry.last_deployed))
            .chain(
                resource
                    .status
                    .conditions
                    .iter()
                    .filter_map(|condition| condition.last_transition_time),
            )
            .max(),
        last_updated: Utc::now(),
        repository: repository(resource),
        branch: branch.map(str::to_string),
        workflow_file: workflow_file(resource),
        resource_state: Some(ResourceState::new(status_label(status), Some(&message))),
        metadata,
    }
}

pub(crate) fn map_available_pipeline(resource: &FluxResource) -> AvailablePipeline {
    let description = [
        Some(resource.kind.to_string()),
        workflow_file(resource),
        source_ref(resource).map(|source| format!("source {source}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" | ");

    AvailablePipeline {
        id: target_of(resource).selection_id(),
        name: display_name(resource),
        description: Some(description),
        organization: Some(resource.metadata.namespace.clone()),
        repository: Some(repository(resource)),
    }
}

#[cfg(test)]
mod tests;
