use chrono::{
    DateTime,
    Duration,
    Utc,
};
use pipedash_plugin_api::logs::content_hash;
use pipedash_plugin_api::{
    Correlation,
    Observed,
    PipelineRun,
    PipelineStatus,
    PluginResult,
    RunEvidence,
};

use super::metadata::{
    base_metadata,
    put,
    split_revision,
};
use super::{
    humanize,
    invalid,
    parse_go_duration,
    Metadata,
};
use crate::types::{
    FluxResource,
    HistoryEntry,
};

pub(crate) const RECORD_HISTORY: &str = "status.history";
const RECONCILIATION_SUCCEEDED: &str = "ReconciliationSucceeded";
pub(super) const MINOR_BASE: i64 = 1_000_000;
const MINOR_SPAN: u64 = 999_999;
const MAX_SECONDS: i64 = 9_000_000_000;
pub(super) const SNAPSHOT_MINOR: i64 = 999_999;

pub(super) struct Candidate {
    pub(super) reason: String,
    pub(super) run: PipelineRun,
}

fn timing_of(recorded: bool) -> Observed {
    if recorded {
        Observed::Recorded
    } else {
        Observed::Absent
    }
}

pub(super) fn seconds_major(time: DateTime<Utc>) -> i64 {
    time.timestamp().clamp(0, MAX_SECONDS) * MINOR_BASE
}

pub(super) fn record_number(time: DateTime<Utc>, minor: u64) -> i64 {
    seconds_major(time) + (minor % MINOR_SPAN) as i64
}

pub(super) fn new_run(
    pipeline_id: &str, status: PipelineStatus, started_at: DateTime<Utc>, metadata: Metadata,
) -> PipelineRun {
    PipelineRun {
        correlation: Correlation::default(),
        evidence: RunEvidence::default(),
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

pub(super) fn kustomization_candidate(
    resource: &FluxResource, pipeline_id: &str, entry: &HistoryEntry,
) -> PluginResult<Candidate> {
    let concluded = entry
        .last_reconciled
        .ok_or_else(|| invalid(resource, "status.history entry without lastReconciled"))?;
    let duration = entry
        .last_reconciled_duration
        .as_deref()
        .and_then(parse_go_duration);
    let derived_start = duration
        .and_then(|duration| Duration::from_std(duration).ok())
        .and_then(|span| concluded.checked_sub_signed(span));
    let (started_at, time_source) = match derived_start {
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
        content_hash(identity.as_bytes()),
    );
    run.id = format!("{pipeline_id}__{}", run.run_number);
    run.concluded_at = Some(concluded);
    run.duration_seconds = duration
        .filter(|_| derived_start.is_some())
        .map(|duration| duration.as_secs_f64().round() as i64);
    run.evidence.timing = timing_of(derived_start.is_some());
    run.branch = branch.map(str::to_string);
    run.commit_sha = commit_sha.map(str::to_string);

    Ok(Candidate { reason, run })
}

pub(super) fn helm_candidate(
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
        || content_hash(entry.digest.as_deref().unwrap_or_default().as_bytes()),
        i64::unsigned_abs,
    );
    let mut run = new_run(pipeline_id, status, deployed, metadata);
    run.run_number = record_number(deployed, minor);
    run.id = format!("{pipeline_id}__{}", run.run_number);
    run.duration_seconds = duration.map(|duration| duration.as_secs_f64().round() as i64);
    run.evidence.timing = timing_of(duration.is_some());

    Ok(Candidate {
        reason: release_status,
        run,
    })
}
