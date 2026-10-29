use std::collections::{
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Duration,
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

use crate::{
    client::REQUESTED_AT_ANNOTATION,
    config::{
        Target,
        PROVIDER_TYPE,
    },
    types::{
        Condition,
        FluxResource,
        HistoryEntry,
        Kind,
        Reference,
    },
};

type Metadata = HashMap<String, Value>;

const HISTORY_SCOPE: &str = "bounded_reconciliation";
const RECORD_SNAPSHOT: &str = "current_snapshot";
const RECORD_HISTORY: &str = "status.history";
const PROGRESSING_WITH_RETRY: &str = "ProgressingWithRetry";
const RECONCILIATION_SUCCEEDED: &str = "ReconciliationSucceeded";
const MINOR_BASE: i64 = 1_000_000;
const MINOR_SPAN: u64 = 999_999;
const SNAPSHOT_MINOR: i64 = 999_999;
const MAX_SECONDS: i64 = 9_000_000_000;

struct Candidate {
    reason: String,
    run: PipelineRun,
}

pub(crate) fn target_of(resource: &FluxResource) -> Target {
    Target {
        kind: resource.kind,
        namespace: resource.metadata.namespace.clone(),
        name: resource.metadata.name.clone(),
    }
}

fn condition<'a>(resource: &'a FluxResource, type_: &str) -> Option<&'a Condition> {
    resource
        .status
        .conditions
        .iter()
        .find(|condition| condition.type_ == type_)
}

fn is_fresh(resource: &FluxResource, condition: &Condition) -> bool {
    condition
        .observed_generation
        .or(resource.status.observed_generation)
        .is_none_or(|observed| observed >= resource.metadata.generation)
}

fn fresh<'a>(resource: &'a FluxResource, type_: &str) -> Option<&'a Condition> {
    condition(resource, type_).filter(|condition| is_fresh(resource, condition))
}

fn asserted<'a>(resource: &'a FluxResource, type_: &str) -> Option<&'a Condition> {
    fresh(resource, type_).filter(|condition| condition.status == "True")
}

fn is_suspended(resource: &FluxResource) -> bool {
    resource.spec.suspend == Some(true)
}

fn is_stale(resource: &FluxResource) -> bool {
    condition(resource, "Ready").is_some_and(|ready| !is_fresh(resource, ready))
}

pub(crate) fn current_status(resource: &FluxResource) -> PipelineStatus {
    if is_suspended(resource) {
        return PipelineStatus::Skipped;
    }

    let ready = fresh(resource, "Ready");
    let reconciling = asserted(resource, "Reconciling");
    let retrying_failure = ready.is_some_and(|ready| ready.status == "False")
        && reconciling.is_some_and(|reconciling| reconciling.reason == PROGRESSING_WITH_RETRY);

    if asserted(resource, "Stalled").is_some() || retrying_failure {
        PipelineStatus::Failed
    } else if reconciling.is_some() {
        PipelineStatus::Running
    } else {
        match ready.map(|ready| ready.status.as_str()) {
            Some("True") => PipelineStatus::Success,
            Some("False") => PipelineStatus::Failed,
            Some(_) => PipelineStatus::Running,
            None => PipelineStatus::Pending,
        }
    }
}

pub(crate) fn status_label(status: PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Success => "Ready",
        PipelineStatus::Failed => "Not ready",
        PipelineStatus::Degraded => "Degraded",
        PipelineStatus::Running => "Reconciling",
        PipelineStatus::Pending => "Pending",
        PipelineStatus::Skipped => "Suspended",
        PipelineStatus::Cancelled => "Cancelled",
    }
}

pub(crate) fn headline(resource: &FluxResource, status: PipelineStatus) -> (String, String) {
    if is_suspended(resource) {
        return (
            "Suspended".to_string(),
            "spec.suspend is true; Flux is not reconciling this resource".to_string(),
        );
    }
    if let Some(ready) = condition(resource, "Ready").filter(|ready| !is_fresh(resource, ready)) {
        let observed = ready
            .observed_generation
            .or(resource.status.observed_generation)
            .map_or_else(|| "unknown".to_string(), |observed| observed.to_string());
        return (
            "StatusOutdated".to_string(),
            format!(
                "Flux reported status for generation {observed}, but the resource is at generation {}",
                resource.metadata.generation
            ),
        );
    }

    let primary = match status {
        PipelineStatus::Running => {
            asserted(resource, "Reconciling").or_else(|| fresh(resource, "Ready"))
        }
        _ => fresh(resource, "Ready")
            .or_else(|| asserted(resource, "Stalled"))
            .or_else(|| asserted(resource, "Reconciling")),
    };

    match primary {
        Some(condition) => (condition.reason.clone(), condition.message.clone()),
        None => (
            "NoStatus".to_string(),
            "Flux has not reported status conditions yet".to_string(),
        ),
    }
}

fn put<V: Into<Value>>(metadata: &mut Metadata, key: &str, value: Option<V>) {
    if let Some(value) = value {
        metadata.insert(key.to_string(), value.into());
    }
}

fn base_metadata(resource: &FluxResource) -> Metadata {
    let mut metadata = Metadata::new();
    put(&mut metadata, "kind", Some(resource.kind.as_str()));
    put(
        &mut metadata,
        "namespace",
        Some(resource.metadata.namespace.as_str()),
    );
    put(&mut metadata, "name", Some(resource.metadata.name.as_str()));
    put(&mut metadata, "uid", resource.metadata.uid.as_deref());
    put(&mut metadata, "history_scope", Some(HISTORY_SCOPE));
    metadata
}

fn source_reference(resource: &FluxResource) -> Option<&Reference> {
    match resource.kind {
        Kind::Kustomization => resource.spec.source_ref.as_ref(),
        Kind::HelmRelease => resource
            .spec
            .chart
            .as_ref()
            .and_then(|chart| chart.spec.source_ref.as_ref())
            .or(resource.spec.chart_ref.as_ref()),
    }
    .filter(|reference| !reference.name.is_empty())
}

fn source_ref(resource: &FluxResource) -> Option<String> {
    source_reference(resource).map(|reference| {
        format!(
            "{}/{}/{}",
            reference.kind,
            reference
                .namespace
                .as_deref()
                .unwrap_or(&resource.metadata.namespace),
            reference.name
        )
    })
}

fn chart(resource: &FluxResource) -> Option<String> {
    if resource.kind != Kind::HelmRelease {
        return None;
    }

    match resource.spec.chart.as_ref() {
        Some(template) if !template.spec.chart.is_empty() => {
            Some(match template.spec.version.as_deref() {
                Some(version) if !version.is_empty() => {
                    format!("{}@{version}", template.spec.chart)
                }
                _ => template.spec.chart.clone(),
            })
        }
        _ => resource
            .spec
            .chart_ref
            .as_ref()
            .map(|reference| reference.name.clone()),
    }
}

fn current_revision(resource: &FluxResource) -> Option<&str> {
    resource
        .status
        .last_attempted_revision
        .as_deref()
        .or(resource.status.last_applied_revision.as_deref())
}

fn latest_release_version(resource: &FluxResource) -> Option<i64> {
    resource
        .status
        .history
        .iter()
        .filter_map(|entry| entry.version)
        .max()
}

fn trim_algorithm(digest: &str) -> &str {
    digest.rsplit_once(':').map_or(digest, |(_, hex)| hex)
}

fn split_revision(revision: &str) -> (Option<&str>, Option<&str>) {
    match revision.split_once('@') {
        Some((reference, digest)) => (
            Some(reference).filter(|reference| !reference.is_empty()),
            Some(trim_algorithm(digest)).filter(|digest| !digest.is_empty()),
        ),
        None => (None, revision.rsplit_once(':').map(|(_, hex)| hex)),
    }
}

fn display_name(resource: &FluxResource) -> String {
    format!("{}/{}", resource.kind, resource.metadata.name)
}

fn repository(resource: &FluxResource) -> String {
    match source_reference(resource) {
        Some(reference) => format!("{}/{}", resource.metadata.namespace, reference.name),
        None => resource.metadata.namespace.clone(),
    }
}

fn workflow_file(resource: &FluxResource) -> Option<String> {
    match resource.kind {
        Kind::Kustomization => resource.spec.path.clone().filter(|path| !path.is_empty()),
        Kind::HelmRelease => chart(resource),
    }
}

fn state_metadata(resource: &FluxResource, reason: String) -> Metadata {
    let ready = condition(resource, "Ready");
    let mut metadata = Metadata::new();
    put(&mut metadata, "suspended", Some(is_suspended(resource)));
    put(&mut metadata, "stale", Some(is_stale(resource)));
    put(&mut metadata, "reason", Some(reason));
    put(
        &mut metadata,
        "ready_status",
        ready.map(|ready| ready.status.as_str()),
    );
    put(
        &mut metadata,
        "ready_reason",
        ready.map(|ready| ready.reason.as_str()),
    );
    put(
        &mut metadata,
        "ready_message",
        ready.map(|ready| ready.message.as_str()),
    );
    put(
        &mut metadata,
        "reconciling_reason",
        asserted(resource, "Reconciling").map(|reconciling| reconciling.reason.as_str()),
    );
    put(
        &mut metadata,
        "stalled_reason",
        asserted(resource, "Stalled").map(|stalled| stalled.reason.as_str()),
    );
    put(
        &mut metadata,
        "generation",
        Some(resource.metadata.generation),
    );
    put(
        &mut metadata,
        "observed_generation",
        resource.status.observed_generation,
    );
    put(&mut metadata, "revision", current_revision(resource));
    put(
        &mut metadata,
        "applied_revision",
        resource.status.last_applied_revision.as_deref(),
    );
    put(&mut metadata, "source_ref", source_ref(resource));
    put(&mut metadata, "chart", chart(resource));
    put(&mut metadata, "interval", resource.spec.interval.as_deref());
    put(
        &mut metadata,
        "release_version",
        latest_release_version(resource),
    );
    put(
        &mut metadata,
        "last_handled_reconcile_at",
        resource.status.last_handled_reconcile_at.as_deref(),
    );
    put(
        &mut metadata,
        "reconcile_requested_at",
        resource
            .metadata
            .annotations
            .get(REQUESTED_AT_ANNOTATION)
            .map(String::as_str),
    );
    metadata
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

fn seconds_major(time: DateTime<Utc>) -> i64 {
    time.timestamp().clamp(0, MAX_SECONDS) * MINOR_BASE
}

fn record_number(time: DateTime<Utc>, minor: u64) -> i64 {
    seconds_major(time) + (minor % MINOR_SPAN) as i64
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn new_run(
    pipeline_id: &str, status: PipelineStatus, started_at: DateTime<Utc>, metadata: Metadata,
) -> PipelineRun {
    PipelineRun {
        id: String::new(),
        pipeline_id: pipeline_id.to_string(),
        run_number: 0,
        status,
        started_at,
        concluded_at: None,
        duration_seconds: None,
        logs_url: String::new(),
        commit_sha: None,
        commit_message: None,
        branch: None,
        actor: None,
        inputs: None,
        metadata,
    }
}

fn kustomization_candidate(
    resource: &FluxResource, pipeline_id: &str, entry: &HistoryEntry,
) -> PluginResult<Candidate> {
    let concluded = entry
        .last_reconciled
        .ok_or_else(|| invalid(resource, "status.history entry without lastReconciled"))?;
    let duration = entry
        .last_reconciled_duration
        .as_deref()
        .and_then(parse_go_duration);
    let (started_at, time_source) = match duration
        .and_then(|duration| Duration::from_std(duration).ok())
        .and_then(|span| concluded.checked_sub_signed(span))
    {
        Some(started) => (
            started,
            "status.history.lastReconciled - lastReconciledDuration",
        ),
        None => (concluded, "status.history.lastReconciled"),
    };
    let succeeded = entry.last_reconciled_status.as_deref() == Some(RECONCILIATION_SUCCEEDED);
    let status = if succeeded {
        PipelineStatus::Success
    } else {
        PipelineStatus::Failed
    };
    let reason = entry.last_reconciled_status.clone().unwrap_or_default();
    let revision = entry.metadata.get("revision").map(String::as_str);
    let (branch, commit_sha) = revision.map(split_revision).unwrap_or_default();

    let mut metadata = base_metadata(resource);
    put(&mut metadata, "record_source", Some(RECORD_HISTORY));
    put(&mut metadata, "time_source", Some(time_source));
    put(
        &mut metadata,
        "run_label",
        Some(match commit_sha {
            Some(sha) => format!("Reconciliation {}", sha.chars().take(7).collect::<String>()),
            None => "Reconciliation".to_string(),
        }),
    );
    put(
        &mut metadata,
        "status_label",
        Some(if succeeded {
            "Reconciled"
        } else {
            "Reconcile failed"
        }),
    );
    put(&mut metadata, "reason", Some(reason.as_str()));
    put(&mut metadata, "revision", revision);
    put(
        &mut metadata,
        "origin_revision",
        entry.metadata.get("originRevision").map(String::as_str),
    );
    put(&mut metadata, "digest", entry.digest.as_deref());
    put(
        &mut metadata,
        "first_reconciled",
        entry.first_reconciled.map(|time| time.to_rfc3339()),
    );
    put(
        &mut metadata,
        "total_reconciliations",
        entry.total_reconciliations,
    );

    let identity = format!("{}|{}", entry.digest.as_deref().unwrap_or_default(), reason);
    let mut run = new_run(pipeline_id, status, started_at, metadata);
    run.run_number = record_number(
        entry.first_reconciled.unwrap_or(concluded),
        fnv1a(&identity),
    );
    run.id = format!("{pipeline_id}__{}", run.run_number);
    run.concluded_at = Some(concluded);
    run.duration_seconds = duration.map(|duration| duration.as_secs_f64().round() as i64);
    run.branch = branch.map(str::to_string);
    run.commit_sha = commit_sha.map(str::to_string);

    Ok(Candidate { reason, run })
}

fn helm_candidate(
    resource: &FluxResource, pipeline_id: &str, entry: &HistoryEntry, latest_version: Option<i64>,
) -> PluginResult<Candidate> {
    let deployed = entry
        .last_deployed
        .or(entry.first_deployed)
        .ok_or_else(|| invalid(resource, "status.history entry without lastDeployed"))?;
    let time_source = if entry.last_deployed.is_some() {
        "status.history.lastDeployed"
    } else {
        "status.history.firstDeployed"
    };
    let release_status = entry.status.clone().unwrap_or_default();
    let status = match release_status.as_str() {
        "deployed" | "superseded" => PipelineStatus::Success,
        "failed" => PipelineStatus::Failed,
        pending if pending.starts_with("pending-") || pending == "uninstalling" => {
            PipelineStatus::Running
        }
        _ => PipelineStatus::Pending,
    };
    let is_latest_attempt = entry.version.is_some()
        && entry.version == latest_version
        && entry.action.is_some()
        && entry.action == resource.status.last_attempted_release_action;
    let duration = resource
        .status
        .last_attempted_release_action_duration
        .as_deref()
        .filter(|_| is_latest_attempt)
        .and_then(parse_go_duration);
    let chart = match (entry.chart_name.as_deref(), entry.chart_version.as_deref()) {
        (Some(name), Some(version)) => Some(format!("{name}@{version}")),
        (Some(name), None) => Some(name.to_string()),
        _ => None,
    };

    let mut metadata = base_metadata(resource);
    put(&mut metadata, "record_source", Some(RECORD_HISTORY));
    put(&mut metadata, "time_source", Some(time_source));
    put(
        &mut metadata,
        "run_label",
        Some(entry.version.map_or_else(
            || "Release".to_string(),
            |version| format!("Release #{version}"),
        )),
    );
    put(
        &mut metadata,
        "status_label",
        Some(if release_status.is_empty() {
            "Unknown".to_string()
        } else {
            humanize(&release_status)
        }),
    );
    put(&mut metadata, "reason", Some(release_status.as_str()));
    put(
        &mut metadata,
        "release_status",
        Some(release_status.as_str()),
    );
    put(&mut metadata, "release_version", entry.version);
    put(&mut metadata, "action", entry.action.as_deref());
    put(&mut metadata, "chart", chart);
    put(&mut metadata, "chart_name", entry.chart_name.as_deref());
    put(
        &mut metadata,
        "chart_version",
        entry.chart_version.as_deref(),
    );
    put(&mut metadata, "revision", entry.chart_version.as_deref());
    put(&mut metadata, "app_version", entry.app_version.as_deref());
    put(&mut metadata, "digest", entry.digest.as_deref());
    put(
        &mut metadata,
        "config_digest",
        entry.config_digest.as_deref(),
    );
    put(
        &mut metadata,
        "first_deployed",
        entry.first_deployed.map(|time| time.to_rfc3339()),
    );

    let minor = entry.version.map_or_else(
        || fnv1a(entry.digest.as_deref().unwrap_or_default()),
        i64::unsigned_abs,
    );
    let mut run = new_run(pipeline_id, status, deployed, metadata);
    run.run_number = record_number(deployed, minor);
    run.id = format!("{pipeline_id}__{}", run.run_number);
    run.duration_seconds = duration.map(|duration| duration.as_secs_f64().round() as i64);

    Ok(Candidate {
        reason: release_status,
        run,
    })
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

pub(crate) fn build_runs(
    resource: &FluxResource, provider_id: i64,
) -> PluginResult<Vec<PipelineRun>> {
    let pipeline_id = target_of(resource).pipeline_id(provider_id);
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

pub(crate) fn map_pipeline(
    resource: &FluxResource, provider_id: i64, workload_issues: &[String],
) -> Pipeline {
    let flux_status = current_status(resource);
    let (reason, flux_message) = headline(resource, flux_status);
    let degraded = match (flux_status, workload_issues.split_first()) {
        (PipelineStatus::Success, Some((first, []))) => Some(first.clone()),
        (PipelineStatus::Success, Some((first, rest))) => {
            Some(format!("{first} (+{} more)", rest.len()))
        }
        _ => None,
    };
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
        id: target_of(resource).pipeline_id(provider_id),
        provider_id,
        provider_type: PROVIDER_TYPE.to_string(),
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
mod tests {
    use serde_json::json;

    use super::*;

    const CREATED: &str = "2026-10-20T10:00:00Z";

    fn resource(kind: Kind, generation: i64, spec: Value, status: Value) -> FluxResource {
        FluxResource::from_value(
            kind,
            json!({
                "metadata": {
                    "name": "app",
                    "namespace": "ci",
                    "uid": "uid-1",
                    "generation": generation,
                    "creationTimestamp": CREATED,
                },
                "spec": spec,
                "status": status,
            }),
        )
        .unwrap()
    }

    fn cond(type_: &str, status: &str, reason: &str, generation: i64, at: &str) -> Value {
        json!({
            "type": type_,
            "status": status,
            "reason": reason,
            "message": format!("{type_} {status} {reason}"),
            "observedGeneration": generation,
            "lastTransitionTime": at,
        })
    }

    fn kustomization(status: Value) -> FluxResource {
        resource(Kind::Kustomization, 2, json!({}), status)
    }

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    #[test]
    fn ready_condition_from_an_older_generation_is_pending_not_success() {
        let stale = kustomization(json!({
            "observedGeneration": 1,
            "conditions": [cond("Ready", "True", "ReconciliationSucceeded", 1, CREATED)],
        }));

        assert_eq!(current_status(&stale), PipelineStatus::Pending);
        assert_eq!(
            headline(&stale, PipelineStatus::Pending).0,
            "StatusOutdated"
        );
    }

    #[test]
    fn condition_generation_wins_over_a_reset_status_generation() {
        let failing = kustomization(json!({
            "observedGeneration": -1,
            "conditions": [
                cond("Reconciling", "True", "ProgressingWithRetry", 2, CREATED),
                cond("Ready", "False", "ArtifactFailed", 2, CREATED),
            ],
        }));

        assert_eq!(current_status(&failing), PipelineStatus::Failed);
        assert_eq!(
            headline(&failing, PipelineStatus::Failed),
            (
                "ArtifactFailed".to_string(),
                "Ready False ArtifactFailed".to_string()
            )
        );
    }

    #[test]
    fn conditions_map_to_ready_reconciling_stalled_and_pending() {
        let cases = [
            (
                vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)],
                PipelineStatus::Success,
            ),
            (
                vec![cond("Ready", "False", "BuildFailed", 2, CREATED)],
                PipelineStatus::Failed,
            ),
            (
                vec![cond("Ready", "Unknown", "Progressing", 2, CREATED)],
                PipelineStatus::Running,
            ),
            (
                vec![
                    cond("Reconciling", "True", "Progressing", 2, CREATED),
                    cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED),
                ],
                PipelineStatus::Running,
            ),
            (
                vec![
                    cond("Stalled", "True", "RetriesExceeded", 2, CREATED),
                    cond("Ready", "False", "InstallFailed", 2, CREATED),
                ],
                PipelineStatus::Failed,
            ),
            (vec![], PipelineStatus::Pending),
        ];

        for (conditions, expected) in cases {
            let resource =
                kustomization(json!({ "observedGeneration": 2, "conditions": conditions }));
            assert_eq!(current_status(&resource), expected, "{conditions:?}");
        }
    }

    #[test]
    fn suspended_resources_are_skipped_even_when_ready() {
        let suspended = resource(
            Kind::HelmRelease,
            2,
            json!({ "suspend": true }),
            json!({ "conditions": [cond("Ready", "True", "InstallSucceeded", 2, CREATED)] }),
        );

        assert_eq!(current_status(&suspended), PipelineStatus::Skipped);
        assert_eq!(status_label(PipelineStatus::Skipped), "Suspended");
        assert_eq!(headline(&suspended, PipelineStatus::Skipped).0, "Suspended");
    }

    #[test]
    fn go_durations_parse_fractions_and_compound_units() {
        let millis = |text| {
            parse_go_duration(text).map(|duration| (duration.as_secs_f64() * 1000.0).round() as i64)
        };

        assert_eq!(millis("2.5s"), Some(2500));
        assert_eq!(millis("1m30s"), Some(90_000));
        assert_eq!(millis("176.163666ms"), Some(176));
        assert_eq!(millis("45.236161127s"), Some(45_236));
        assert_eq!(millis("1h"), Some(3_600_000));
        assert_eq!(millis("fast"), None);
        assert_eq!(millis(""), None);
    }

    #[test]
    fn revisions_split_into_reference_and_commit_digest() {
        assert_eq!(
            split_revision("6.7.1@sha1:6b7aab8a"),
            (Some("6.7.1"), Some("6b7aab8a"))
        );
        assert_eq!(split_revision("sha256:abcd"), (None, Some("abcd")));
        assert_eq!(split_revision("6.7.1"), (None, None));
    }

    fn kustomization_with_history(history: Value, conditions: Vec<Value>) -> FluxResource {
        kustomization(json!({
            "observedGeneration": 2,
            "history": history,
            "conditions": conditions,
        }))
    }

    fn entry(digest: &str, first: &str, last: &str, duration: &str, status: &str) -> Value {
        json!({
            "digest": digest,
            "firstReconciled": first,
            "lastReconciled": last,
            "lastReconciledDuration": duration,
            "lastReconciledStatus": status,
            "totalReconciliations": 3,
            "metadata": { "revision": "6.7.1@sha1:6b7aab8a" },
        })
    }

    fn number_of(runs: &[PipelineRun], digest: &str) -> Option<i64> {
        runs.iter()
            .find(|run| {
                run.metadata
                    .get("digest")
                    .is_some_and(|value| value == digest)
            })
            .map(|run| run.run_number)
    }

    fn digests(runs: &[PipelineRun]) -> Vec<&str> {
        runs.iter()
            .filter_map(|run| run.metadata.get("digest")?.as_str())
            .collect()
    }

    #[test]
    fn kustomization_history_keeps_native_times_durations_and_labels() {
        let resource = kustomization_with_history(
            json!([
                entry(
                    "sha256:b",
                    "2026-10-21T10:00:00Z",
                    "2026-10-21T12:00:45Z",
                    "45s",
                    "ReconciliationSucceeded"
                ),
                entry(
                    "sha256:a",
                    "2026-10-20T10:00:00Z",
                    "2026-10-20T10:00:10Z",
                    "10s",
                    "HealthCheckFailed"
                ),
            ]),
            vec![cond(
                "Ready",
                "True",
                "ReconciliationSucceeded",
                2,
                "2026-10-21T12:00:45Z",
            )],
        );

        let runs = build_runs(&resource, 3).unwrap();

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].status, PipelineStatus::Success);
        assert_eq!(
            runs[0].run_number / 1_000_000,
            at("2026-10-21T10:00:00Z").timestamp()
        );
        assert_eq!(runs[0].started_at, at("2026-10-21T12:00:00Z"));
        assert_eq!(runs[0].concluded_at, Some(at("2026-10-21T12:00:45Z")));
        assert_eq!(runs[0].duration_seconds, Some(45));
        assert_eq!(runs[0].commit_sha.as_deref(), Some("6b7aab8a"));
        assert_eq!(runs[0].metadata["total_reconciliations"], json!(3));
        assert_eq!(
            runs[0].metadata["message"],
            json!("Ready True ReconciliationSucceeded")
        );
        assert_eq!(runs[1].status, PipelineStatus::Failed);
        assert_eq!(runs[1].metadata["status_label"], json!("Reconcile failed"));
        assert_eq!(runs[1].pipeline_id, "fluxcd__3__Kustomization__ci__app");
        assert!(runs[1]
            .id
            .starts_with("fluxcd__3__Kustomization__ci__app__"));
    }

    #[test]
    fn current_snapshot_is_added_only_when_history_does_not_cover_it() {
        let history = json!([entry(
            "sha256:a",
            "2026-10-20T10:00:00Z",
            "2026-10-20T10:00:10Z",
            "10s",
            "ReconciliationSucceeded"
        )]);
        let failing = kustomization_with_history(
            history.clone(),
            vec![cond(
                "Ready",
                "False",
                "ArtifactFailed",
                2,
                "2026-10-21T09:00:00Z",
            )],
        );

        let runs = build_runs(&failing, 1).unwrap();

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].metadata["record_source"], json!("current_snapshot"));
        assert_eq!(
            runs[0].metadata["time_source"],
            json!("condition.lastTransitionTime")
        );
        assert_eq!(runs[0].started_at, at("2026-10-21T09:00:00Z"));
        assert_eq!(runs[0].concluded_at, None);
        assert_eq!(runs[0].duration_seconds, None);
        assert_eq!(
            runs[0].run_number,
            runs[1].run_number / 1_000_000 * 1_000_000 + 999_999
        );
        assert_eq!(
            runs[0].metadata["message"],
            json!("Ready False ArtifactFailed")
        );
    }

    #[test]
    fn snapshot_number_ignores_condition_message_and_transition_churn() {
        let first = kustomization(json!({
            "observedGeneration": 2,
            "conditions": [cond("Ready", "False", "ArtifactFailed", 2, "2026-10-21T09:00:00Z")],
        }));
        let retried = kustomization(json!({
            "observedGeneration": 2,
            "conditions": [{
                "type": "Ready", "status": "False", "reason": "ArtifactFailed",
                "message": "path not found: /tmp/other-dir", "observedGeneration": 2,
                "lastTransitionTime": "2026-10-21T09:01:00Z",
            }],
        }));

        let before = build_runs(&first, 1).unwrap();
        let after = build_runs(&retried, 1).unwrap();

        assert_eq!(before.len(), 1);
        assert_eq!(before[0].run_number, after[0].run_number);
        assert_eq!(before[0].id, after[0].id);
        assert_eq!(
            before[0].run_number,
            at(CREATED).timestamp() * 1_000_000 + 999_999
        );
    }

    #[test]
    fn same_second_records_keep_their_identity_when_a_tied_record_is_evicted() {
        let a = entry(
            "sha256:a",
            "2026-10-20T10:00:00Z",
            "2026-10-20T10:00:10Z",
            "1s",
            "ReconciliationSucceeded",
        );
        let b = entry(
            "sha256:b",
            "2026-10-20T10:00:00Z",
            "2026-10-20T10:00:05Z",
            "1s",
            "HealthCheckFailed",
        );
        let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];

        let forward =
            build_runs(&kustomization_with_history(json!([a, b]), ready.clone()), 1).unwrap();
        let reverse =
            build_runs(&kustomization_with_history(json!([b, a]), ready.clone()), 1).unwrap();
        let evicted = build_runs(&kustomization_with_history(json!([b]), ready), 1).unwrap();

        assert_ne!(
            number_of(&forward, "sha256:a"),
            number_of(&forward, "sha256:b")
        );
        assert_eq!(
            number_of(&forward, "sha256:a"),
            number_of(&reverse, "sha256:a")
        );
        assert_eq!(
            number_of(&forward, "sha256:b"),
            number_of(&reverse, "sha256:b")
        );
        assert_eq!(
            number_of(&forward, "sha256:b"),
            number_of(&evicted, "sha256:b")
        );
        assert_eq!(number_of(&evicted, "sha256:a"), None);
    }

    #[test]
    fn history_lists_the_latest_reconciliation_first_without_changing_identities() {
        let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];
        let reconciled = |last: &str| {
            entry(
                "sha256:a",
                "2026-10-10T10:00:00Z",
                last,
                "10s",
                "ReconciliationSucceeded",
            )
        };
        let middle = entry(
            "sha256:b",
            "2026-10-15T10:00:00Z",
            "2026-10-16T10:00:10Z",
            "10s",
            "ReconciliationSucceeded",
        );
        let build = |history: Vec<Value>| {
            build_runs(
                &kustomization_with_history(json!(history), ready.clone()),
                1,
            )
            .unwrap()
        };

        let before = build(vec![reconciled("2026-10-14T10:00:10Z"), middle.clone()]);
        let after = build(vec![middle, reconciled("2026-10-22T10:00:10Z")]);
        let evicted = build(vec![reconciled("2026-10-22T10:00:10Z")]);

        assert_eq!(digests(&before), ["sha256:b", "sha256:a"]);
        assert_eq!(digests(&after), ["sha256:a", "sha256:b"]);
        assert!(number_of(&after, "sha256:a") < number_of(&after, "sha256:b"));
        assert_eq!(
            number_of(&before, "sha256:a"),
            number_of(&after, "sha256:a")
        );
        assert_eq!(
            number_of(&before, "sha256:b"),
            number_of(&after, "sha256:b")
        );
        assert_eq!(
            number_of(&evicted, "sha256:a"),
            number_of(&after, "sha256:a")
        );
        assert_eq!(number_of(&evicted, "sha256:b"), None);
    }

    #[test]
    fn records_observed_at_the_same_time_order_by_identity_in_any_input_order() {
        let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];
        let record = |digest: &str| {
            entry(
                digest,
                "2026-10-20T10:00:00Z",
                "2026-10-21T10:00:10Z",
                "10s",
                "ReconciliationSucceeded",
            )
        };
        let (a, b) = (record("sha256:a"), record("sha256:b"));

        let forward =
            build_runs(&kustomization_with_history(json!([a, b]), ready.clone()), 1).unwrap();
        let reverse = build_runs(&kustomization_with_history(json!([b, a]), ready), 1).unwrap();

        assert_eq!(forward[0].started_at, forward[1].started_at);
        assert!(forward[0].run_number > forward[1].run_number);
        assert_eq!(digests(&forward), digests(&reverse));
    }

    #[test]
    fn indistinguishable_history_records_are_an_error_not_an_alias() {
        let twin = entry(
            "sha256:a",
            "2026-10-20T10:00:00Z",
            "2026-10-20T10:00:10Z",
            "1s",
            "ReconciliationSucceeded",
        );
        let resource = kustomization_with_history(
            json!([twin, twin]),
            vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)],
        );

        let error = build_runs(&resource, 1).unwrap_err();

        assert!(
            matches!(&error, PluginError::SerializationError(message) if message.contains("Kustomization ci/app")),
            "{error:?}"
        );
    }

    #[test]
    fn repeated_reconciliations_update_the_same_record_instead_of_creating_a_new_one() {
        let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];
        let earlier = entry(
            "sha256:a",
            "2026-10-20T10:00:00Z",
            "2026-10-20T10:00:10Z",
            "1s",
            "ReconciliationSucceeded",
        );
        let mut later = earlier.clone();
        later["lastReconciled"] = json!("2026-10-22T10:00:10Z");
        later["totalReconciliations"] = json!(9);

        let before = build_runs(
            &kustomization_with_history(json!([earlier]), ready.clone()),
            1,
        )
        .unwrap();
        let after = build_runs(&kustomization_with_history(json!([later]), ready), 1).unwrap();

        assert_eq!(before[0].id, after[0].id);
        assert_eq!(after[0].metadata["total_reconciliations"], json!(9));
        assert_ne!(before[0].concluded_at, after[0].concluded_at);
    }

    #[test]
    fn helm_history_exposes_release_versions_and_deploy_times() {
        let resource = resource(
            Kind::HelmRelease,
            1,
            json!({ "chart": { "spec": { "chart": "podinfo", "version": "6.7.1", "sourceRef": { "kind": "HelmRepository", "name": "podinfo" } } } }),
            json!({
                "observedGeneration": 1,
                "lastAttemptedReleaseAction": "upgrade",
                "lastAttemptedReleaseActionDuration": "30.3s",
                "history": [
                    { "version": 2, "action": "upgrade", "status": "failed", "chartName": "podinfo", "chartVersion": "6.7.2", "digest": "sha256:2", "firstDeployed": "2026-10-20T10:00:00Z", "lastDeployed": "2026-10-21T10:00:00Z" },
                    { "version": 1, "action": "install", "status": "superseded", "chartName": "podinfo", "chartVersion": "6.7.1", "digest": "sha256:1", "firstDeployed": "2026-10-20T10:00:00Z", "lastDeployed": "2026-10-20T10:00:00Z" },
                ],
                "conditions": [cond("Ready", "False", "UpgradeFailed", 1, "2026-10-21T10:00:31Z")],
            }),
        );

        let runs = build_runs(&resource, 1).unwrap();

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].metadata["release_version"], json!(2));
        assert_eq!(runs[0].run_number % 1_000_000, 2);
        assert_eq!(runs[1].run_number % 1_000_000, 1);
        assert_eq!(
            runs[0].run_number / 1_000_000,
            at("2026-10-21T10:00:00Z").timestamp()
        );
        assert_eq!(runs[0].metadata["chart"], json!("podinfo@6.7.2"));
        assert_eq!(runs[0].status, PipelineStatus::Failed);
        assert_eq!(runs[0].started_at, at("2026-10-21T10:00:00Z"));
        assert_eq!(runs[0].duration_seconds, Some(30));
        assert_eq!(
            runs[0].metadata["message"],
            json!("Ready False UpgradeFailed")
        );
        assert_eq!(runs[1].status, PipelineStatus::Success);
        assert_eq!(runs[1].metadata["status_label"], json!("Superseded"));
        assert_eq!(runs[1].duration_seconds, None);
    }

    #[test]
    fn pipeline_groups_by_namespace_and_source_with_kind_in_the_name() {
        let resource = resource(
            Kind::Kustomization,
            1,
            json!({ "path": "./kustomize", "suspend": true, "sourceRef": { "kind": "GitRepository", "name": "podinfo" } }),
            json!({}),
        );
        let pipeline = map_pipeline(&resource, 9, &[]);

        assert_eq!(pipeline.id, "fluxcd__9__Kustomization__ci__app");
        assert_eq!(pipeline.name, "Kustomization/app");
        assert_eq!(pipeline.repository, "ci/podinfo");
        assert_eq!(pipeline.status, PipelineStatus::Skipped);
        assert_eq!(pipeline.metadata["suspended"], json!(true));
        assert_eq!(
            pipeline.resource_state,
            Some(ResourceState::new(
                "Suspended",
                Some("spec.suspend is true; Flux is not reconciling this resource")
            ))
        );
        assert!(!pipeline.metadata.contains_key("status_label"));
        assert!(!pipeline.metadata.contains_key("message"));
        assert_eq!(pipeline.metadata["reason"], json!("Suspended"));
        assert_eq!(
            pipeline.metadata["source_ref"],
            json!("GitRepository/ci/podinfo")
        );
        assert_eq!(
            pipeline.metadata["history_scope"],
            json!("bounded_reconciliation")
        );
    }

    fn ready(status: &str) -> FluxResource {
        kustomization(json!({
            "observedGeneration": 2,
            "conditions": [cond("Ready", status, "ReconciliationSucceeded", 2, CREATED)],
        }))
    }

    #[test]
    fn workload_issues_degrade_only_a_ready_resource_and_summarize_extras() {
        let issues = vec![
            "Deployment apps/web: 1 of 3 replicas ready".to_string(),
            "StatefulSet apps/db: 0 of 1 replicas ready".to_string(),
            "DaemonSet apps/agent: 2 of 5 pods ready".to_string(),
        ];

        let degraded = map_pipeline(&ready("True"), 1, &issues[..1]);
        assert_eq!(degraded.status, PipelineStatus::Degraded);
        assert_eq!(
            degraded.resource_state,
            Some(ResourceState::new(
                "Degraded",
                Some("Deployment apps/web: 1 of 3 replicas ready")
            ))
        );
        assert_eq!(
            degraded.metadata["reason"],
            json!("ReconciliationSucceeded")
        );

        let several = map_pipeline(&ready("True"), 1, &issues);
        assert_eq!(
            several.resource_state.unwrap().message.as_deref(),
            Some("Deployment apps/web: 1 of 3 replicas ready (+2 more)")
        );

        for (status, expected, label) in [
            ("False", PipelineStatus::Failed, "Not ready"),
            ("Unknown", PipelineStatus::Running, "Reconciling"),
        ] {
            let pipeline = map_pipeline(&ready(status), 1, &issues);
            assert_eq!(pipeline.status, expected);
            assert_eq!(pipeline.resource_state.unwrap().label, label);
        }

        let healthy = map_pipeline(&ready("True"), 1, &[]);
        assert_eq!(healthy.status, PipelineStatus::Success);
        assert_eq!(
            healthy.resource_state,
            Some(ResourceState::new(
                "Ready",
                Some("Ready True ReconciliationSucceeded")
            ))
        );
    }

    #[test]
    fn snapshot_run_keeps_label_and_message_in_run_metadata() {
        let runs = build_runs(&ready("False"), 1).unwrap();

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].metadata["status_label"], json!("Not ready"));
        assert_eq!(
            runs[0].metadata["message"],
            json!("Ready False ReconciliationSucceeded")
        );
        assert_eq!(runs[0].metadata["reason"], json!("ReconciliationSucceeded"));
    }

    #[test]
    fn run_numbers_stay_within_javascript_safe_integers() {
        let resource = kustomization_with_history(
            json!([entry(
                "sha256:a",
                "9999-12-31T23:59:59Z",
                "9999-12-31T23:59:59Z",
                "1s",
                "ReconciliationSucceeded"
            )]),
            vec![cond("Ready", "False", "BuildFailed", 2, CREATED)],
        );

        let runs = build_runs(&resource, 1).unwrap();

        assert!(runs.iter().all(|run| run.run_number < (1 << 53)));
    }
}
