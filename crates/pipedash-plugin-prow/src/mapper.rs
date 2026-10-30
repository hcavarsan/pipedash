use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Organization,
    Pipeline,
    PipelineCatalog,
    PipelineRun,
    PipelineStatus,
    ResourceState,
    READ_ONLY_METADATA_KEY,
};
use reqwest::Url;
use serde_json::{
    json,
    Value,
};

use crate::config::{
    self,
    Scope,
    Selection,
};
use crate::storage::{
    Artifacts,
    StoredBuild,
};
use crate::types::{
    run_number_of,
    ProwJob,
};

pub(crate) const LOG_NODE: &str = "build-log";

pub(crate) fn map_state(state: &str) -> PipelineStatus {
    match state {
        "pending" => PipelineStatus::Running,
        "success" => PipelineStatus::Success,
        "failure" | "error" => PipelineStatus::Failed,
        "aborted" => PipelineStatus::Cancelled,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn state_label(state: &str) -> Option<&'static str> {
    match state {
        "scheduling" => Some("Scheduling"),
        "triggered" => Some("Triggered"),
        "error" => Some("Error"),
        "aborted" => Some("Aborted"),
        _ => None,
    }
}

pub(crate) fn map_result(
    result: &str, passed: Option<bool>,
) -> (PipelineStatus, Option<&'static str>) {
    match result {
        "SUCCESS" => (PipelineStatus::Success, None),
        "FAILURE" => (PipelineStatus::Failed, None),
        "ABORTED" => (PipelineStatus::Cancelled, Some("Aborted")),
        "ERROR" => (PipelineStatus::Failed, Some("Error")),
        _ if passed == Some(true) => (PipelineStatus::Success, None),
        _ => (PipelineStatus::Failed, None),
    }
}

pub(crate) fn scope_of(job: &ProwJob) -> Option<Scope> {
    job.primary_refs()
        .filter(|refs| !refs.org.is_empty() && !refs.repo.is_empty())
        .map(|refs| Scope {
            org: refs.org.clone(),
            repo: refs.repo.clone(),
        })
}

pub(crate) fn newest<'a>(jobs: &'a [ProwJob], name: &str) -> Option<&'a ProwJob> {
    jobs.iter()
        .filter(|job| job.spec.job == name)
        .max_by_key(|job| (job.status.start_time, job.run_number()))
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn insert_text(metadata: &mut HashMap<String, Value>, key: &str, value: &str) {
    if let Some(value) = non_empty(value) {
        metadata.insert(key.to_string(), Value::String(value));
    }
}

fn duration(started: DateTime<Utc>, finished: Option<DateTime<Utc>>) -> Option<i64> {
    finished.map(|finished| (finished - started).num_seconds().max(0))
}

fn deck_root(deck: &Url) -> &str {
    deck.as_str().trim_end_matches('/')
}

pub(crate) fn map_run(job: &ProwJob, pipeline_id: &str, deck: &Url) -> Option<PipelineRun> {
    let run_number = job.run_number()?;
    let started_at = job.status.start_time.or(job.metadata.creation_timestamp)?;
    let concluded_at = job.status.completion_time;
    let refs = job.primary_refs();
    let pull = refs.and_then(|refs| refs.pulls.first());

    let mut metadata = HashMap::new();
    insert_text(&mut metadata, "build_id", &job.status.build_id);
    insert_text(&mut metadata, "prowjob", &job.metadata.name);
    insert_text(&mut metadata, "job_type", &job.spec.job_type);
    insert_text(&mut metadata, "state", &job.status.state);
    insert_text(&mut metadata, "cluster", &job.spec.cluster);
    insert_text(&mut metadata, "description", &job.status.description);
    if let Some(pull) = pull {
        if pull.number > 0 {
            metadata.insert("pull".to_string(), json!(pull.number));
        }
        insert_text(&mut metadata, "pull_url", &pull.link);
    }
    if let Some(artifacts) = Artifacts::from_view_url(&job.status.url) {
        metadata.insert(
            "artifacts".to_string(),
            Value::String(artifacts.to_metadata()),
        );
    }
    if let Some(label) = state_label(&job.status.state) {
        metadata.insert("status_label".to_string(), json!(label));
    }

    Some(PipelineRun {
        id: job.status.build_id.clone(),
        pipeline_id: pipeline_id.to_string(),
        run_number,
        status: map_state(&job.status.state),
        started_at,
        concluded_at,
        duration_seconds: duration(started_at, concluded_at),
        logs_url: non_empty(&job.status.url).unwrap_or_else(|| {
            format!("{}/prowjob?prowjob={}", deck_root(deck), job.metadata.name)
        }),
        commit_sha: pull
            .and_then(|pull| non_empty(&pull.sha))
            .or_else(|| refs.and_then(|refs| non_empty(&refs.base_sha))),
        commit_message: pull.and_then(|pull| non_empty(&pull.title)),
        branch: refs.and_then(|refs| non_empty(&refs.base_ref)),
        actor: pull.and_then(|pull| non_empty(&pull.author)),
        inputs: Some(json!({ "prowjob": job.metadata.name })),
        metadata,
    })
}

pub(crate) fn runs(
    jobs: &[ProwJob], name: &str, pipeline_id: &str, deck: &Url,
) -> Vec<PipelineRun> {
    let mut runs: Vec<PipelineRun> = jobs
        .iter()
        .filter(|job| job.spec.job == name)
        .filter_map(|job| map_run(job, pipeline_id, deck))
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.run_number));
    runs.dedup_by_key(|run| run.run_number);
    runs
}

fn timestamp(seconds: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(seconds, 0).filter(|_| seconds > 0)
}

pub(crate) fn map_stored_run(
    build: &StoredBuild, pipeline_id: &str, scope: Option<&Scope>, deck: &Url,
) -> PipelineRun {
    let (status, label) = map_result(&build.finished.result, build.finished.passed);
    let started_at = timestamp(build.started.timestamp).unwrap_or_else(Utc::now);
    let concluded_at = build.finished.timestamp.and_then(timestamp);
    let branch = scope
        .and_then(|scope| {
            build
                .started
                .repos
                .get(&format!("{}/{}", scope.org, scope.repo))
        })
        .and_then(|value| value.split(':').next())
        .and_then(non_empty);

    let mut metadata = HashMap::new();
    metadata.insert(
        "build_id".to_string(),
        Value::String(build.number.to_string()),
    );
    metadata.insert(
        "artifacts".to_string(),
        Value::String(build.artifacts.to_metadata()),
    );
    insert_text(&mut metadata, "result", &build.finished.result);
    if let Some(label) = label {
        metadata.insert("status_label".to_string(), json!(label));
    }

    PipelineRun {
        id: build.number.to_string(),
        pipeline_id: pipeline_id.to_string(),
        run_number: run_number_of(build.number),
        status,
        started_at,
        concluded_at,
        duration_seconds: duration(started_at, concluded_at),
        logs_url: build.artifacts.view_url(deck),
        commit_sha: non_empty(&build.started.repo_commit),
        commit_message: None,
        branch,
        actor: None,
        inputs: None,
        metadata,
    }
}

pub(crate) enum Latest<'a> {
    Deck(&'a ProwJob),
    Storage(&'a StoredBuild),
    None,
}

pub(crate) fn map_pipeline(
    provider_id: i64, selection: &Selection, latest: Latest<'_>, deck: &Url, read_only: bool,
) -> Pipeline {
    let (status, last_run) = match &latest {
        Latest::Deck(job) => (map_state(&job.status.state), job.status.start_time),
        Latest::Storage(build) => (
            map_result(&build.finished.result, build.finished.passed).0,
            timestamp(build.started.timestamp),
        ),
        Latest::None => (PipelineStatus::Pending, None),
    };
    let mut metadata = HashMap::new();
    let mut branch = None;
    if let Latest::Deck(job) = &latest {
        insert_text(&mut metadata, "job_type", &job.spec.job_type);
        insert_text(&mut metadata, "cluster", &job.spec.cluster);
        branch = job
            .primary_refs()
            .and_then(|refs| non_empty(&refs.base_ref));
    }
    if read_only {
        metadata.insert(READ_ONLY_METADATA_KEY.to_string(), Value::Bool(true));
    }

    Pipeline {
        id: config::build_pipeline_id(provider_id, selection),
        provider_id,
        provider_type: config::PROVIDER_TYPE.to_string(),
        name: selection.job.clone(),
        status,
        last_run,
        last_updated: Utc::now(),
        repository: match &selection.scope {
            Some(scope) => scope.repository(),
            None => deck.host_str().unwrap_or_default().to_string(),
        },
        branch,
        workflow_file: None,
        resource_state: matches!(latest, Latest::None).then(|| {
            ResourceState::new("No recent runs", Some("Prow has no recent run of this job"))
        }),
        metadata,
    }
}

fn map_available(job: &ProwJob, deck_host: &str) -> AvailablePipeline {
    let scope = scope_of(job);
    let (organization, repository) = match &scope {
        Some(scope) => (scope.org.clone(), scope.repository()),
        None => (deck_host.to_string(), deck_host.to_string()),
    };
    AvailablePipeline {
        id: Selection {
            scope,
            job: job.spec.job.clone(),
        }
        .token(),
        name: job.spec.job.clone(),
        description: Some(format!("{} job", job.spec.job_type)),
        organization: Some(organization),
        repository: Some(repository),
    }
}

pub(crate) fn catalog(jobs: &[ProwJob], deck_host: &str) -> PipelineCatalog {
    let mut newest_first: Vec<&ProwJob> =
        jobs.iter().filter(|job| !job.spec.job.is_empty()).collect();
    newest_first.sort_by_key(|job| std::cmp::Reverse(job.status.start_time));

    let mut seen = HashSet::new();
    let mut pipelines: Vec<AvailablePipeline> = newest_first
        .into_iter()
        .map(|job| map_available(job, deck_host))
        .filter(|pipeline| seen.insert(pipeline.id.clone()))
        .collect();
    pipelines.sort_by(|a, b| a.name.cmp(&b.name));

    let organizations = pipelines
        .iter()
        .filter_map(|pipeline| pipeline.organization.as_deref())
        .collect::<BTreeSet<&str>>()
        .into_iter()
        .map(|org| Organization {
            id: org.to_string(),
            name: config::display_org(org).to_string(),
            description: None,
        })
        .collect();

    PipelineCatalog {
        organizations,
        pipelines,
    }
}

pub(crate) fn log_node(
    status: PipelineStatus, availability: LogAvailability, web_url: Option<String>,
) -> LogNode {
    LogNode {
        id: LOG_NODE.to_string(),
        parent_id: None,
        kind: LogNodeKind::Job,
        name: "Build log".to_string(),
        status,
        started_at: None,
        finished_at: None,
        log: Some(availability),
        web_url,
    }
}
