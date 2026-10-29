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
    provider_id: i64, organization: &str, project: &str, definition: &types::Definition,
) -> Pipeline {
    let latest_build = definition.latest_build.as_ref();

    Pipeline {
        id: config::build_pipeline_id(provider_id, project, definition.id),
        provider_id,
        provider_type: config::PROVIDER_TYPE.to_string(),
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
        id: format!("{}-run-{}", config::PROVIDER_TYPE, build.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: build.id,
        status: map_build_status(build),
        started_at: started_at
            .or_else(|| timestamp(build.queue_time))
            .unwrap_or_else(Utc::now),
        concluded_at,
        duration_seconds: started_at
            .zip(concluded_at)
            .map(|(started, concluded)| (concluded - started).num_seconds()),
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

pub(crate) fn job_log_id(timeline: &types::Timeline, record_id: &str) -> Option<i64> {
    timeline
        .records
        .iter()
        .find(|record| record.id == record_id && record.kind.as_deref() == Some("Job"))
        .and_then(|record| record.log.as_ref())
        .map(|log| log.id)
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
mod tests {
    use super::*;

    const STATUSES: [&str; 7] = [
        "none",
        "inProgress",
        "completed",
        "cancelling",
        "postponed",
        "notStarted",
        "all",
    ];
    const RESULTS: [&str; 5] = [
        "none",
        "succeeded",
        "partiallySucceeded",
        "failed",
        "canceled",
    ];

    fn server() -> Url {
        Url::parse("https://dev.azure.com/acme/").unwrap()
    }

    fn build(json: &str) -> types::Build {
        serde_json::from_str(json).unwrap()
    }

    fn sorted(words: impl IntoIterator<Item = &'static str>) -> Vec<&'static str> {
        let mut words: Vec<_> = words.into_iter().collect();
        words.sort_unstable();
        words
    }

    #[test]
    fn completed_builds_map_by_result() {
        let cases = [
            ("succeeded", PipelineStatus::Success),
            ("partiallySucceeded", PipelineStatus::Failed),
            ("failed", PipelineStatus::Failed),
            ("canceled", PipelineStatus::Cancelled),
            ("none", PipelineStatus::Failed),
        ];
        for (result, expected) in cases {
            assert_eq!(
                map_status(Some("completed"), Some(result)),
                expected,
                "completed/{result}"
            );
        }
        assert_eq!(map_status(Some("completed"), None), PipelineStatus::Failed);
        assert_eq!(
            sorted(cases.iter().map(|(result, _)| *result)),
            sorted(RESULTS)
        );
    }

    #[test]
    fn unfinished_builds_map_by_status_whatever_the_result() {
        let cases = [
            ("none", PipelineStatus::Pending),
            ("inProgress", PipelineStatus::Running),
            ("cancelling", PipelineStatus::Running),
            ("postponed", PipelineStatus::Pending),
            ("notStarted", PipelineStatus::Pending),
            ("all", PipelineStatus::Pending),
        ];
        for (status, expected) in cases {
            for result in RESULTS {
                assert_eq!(
                    map_status(Some(status), Some(result)),
                    expected,
                    "{status}/{result}"
                );
            }
            assert_eq!(map_status(Some(status), None), expected, "{status}");
        }
        assert_eq!(
            sorted(cases.iter().map(|(status, _)| *status).chain(["completed"])),
            sorted(STATUSES)
        );
    }

    #[test]
    fn missing_or_unknown_status_words_stay_pending() {
        assert_eq!(map_status(None, None), PipelineStatus::Pending);
        assert_eq!(
            map_status(Some("somethingNew"), Some("succeeded")),
            PipelineStatus::Pending
        );
    }

    #[test]
    fn definition_takes_its_status_branch_and_time_from_the_latest_build() {
        let definition: types::Definition = serde_json::from_str(
            r#"{
                "id": 12, "name": "CI",
                "latestBuild": {
                    "id": 900, "status": "completed", "result": "succeeded",
                    "sourceBranch": "refs/heads/release/1.0",
                    "queueTime": "2026-10-28T09:59:00.1234567Z",
                    "startTime": "2026-10-28T10:00:00.1234567Z"
                }
            }"#,
        )
        .unwrap();

        let pipeline = map_pipeline(3, "acme", "Web App", &definition);

        assert_eq!(pipeline.id, "azure-devops__3__Web App/12");
        assert_eq!(pipeline.provider_type, "azure-devops");
        assert_eq!(pipeline.name, "CI");
        assert_eq!(pipeline.status, PipelineStatus::Success);
        assert_eq!(pipeline.repository, "acme/Web App");
        assert_eq!(pipeline.branch.as_deref(), Some("release/1.0"));
        assert_eq!(
            pipeline.last_run.unwrap().to_rfc3339(),
            "2026-10-28T10:00:00.123456700+00:00"
        );
    }

    #[test]
    fn definition_without_builds_is_pending() {
        let definition: types::Definition =
            serde_json::from_str(r#"{"id": 1, "name": "New"}"#).unwrap();
        let pipeline = map_pipeline(1, "acme", "App", &definition);
        assert_eq!(pipeline.status, PipelineStatus::Pending);
        assert_eq!(pipeline.last_run, None);
        assert_eq!(pipeline.branch, None);
    }

    #[test]
    fn run_carries_branch_commit_actor_times_link_and_variables() {
        let run = build(
            r#"{
                "id": 901, "buildNumber": "20261028.2",
                "status": "completed", "result": "failed",
                "sourceBranch": "refs/heads/main", "sourceVersion": "abc123def",
                "queueTime": "2026-10-28T09:59:50Z",
                "startTime": "2026-10-28T10:00:00Z", "finishTime": "2026-10-28T10:02:30Z",
                "requestedFor": {"displayName": "Ada Lovelace"},
                "parameters": "{\"deployTarget\":\"staging\"}",
                "_links": {"web": {"href": "https://dev.azure.com/acme/App/_build/results?buildId=901"}}
            }"#,
        );

        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());

        assert_eq!(mapped.id, "azure-devops-run-901");
        assert_eq!(mapped.run_number, 901);
        assert_eq!(mapped.status, PipelineStatus::Failed);
        assert_eq!(mapped.branch.as_deref(), Some("main"));
        assert_eq!(mapped.commit_sha.as_deref(), Some("abc123def"));
        assert_eq!(mapped.actor.as_deref(), Some("Ada Lovelace"));
        assert_eq!(mapped.duration_seconds, Some(150));
        assert!(mapped.concluded_at.is_some());
        assert_eq!(
            mapped.logs_url,
            "https://dev.azure.com/acme/App/_build/results?buildId=901"
        );
        assert_eq!(
            mapped.inputs,
            Some(json!({"source-branch": "main", "deployTarget": "staging"}))
        );
        assert_eq!(mapped.metadata["build_number"], json!("20261028.2"));
    }

    #[test]
    fn run_keeps_non_branch_refs_whole() {
        let run = build(r#"{"id": 5, "sourceBranch": "refs/pull/17/merge"}"#);
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());
        assert_eq!(mapped.branch.as_deref(), Some("refs/pull/17/merge"));
    }

    #[test]
    fn run_with_only_an_id_still_maps() {
        let run = build(r#"{"id": 5}"#);
        let mapped = map_run(&run, "azure-devops__1__My App/12", "My App", &server());

        assert_eq!(mapped.status, PipelineStatus::Pending);
        assert_eq!(mapped.concluded_at, None);
        assert_eq!(mapped.duration_seconds, None);
        assert_eq!(mapped.branch, None);
        assert_eq!(mapped.commit_sha, None);
        assert_eq!(mapped.actor, None);
        assert_eq!(mapped.inputs, None);
        assert!(mapped.metadata.is_empty());
        assert_eq!(
            mapped.logs_url,
            "https://dev.azure.com/acme/My%20App/_build/results?buildId=5"
        );
    }

    #[test]
    fn unset_minimum_date_timestamps_are_treated_as_missing() {
        let run = build(
            r#"{"id": 6, "status": "notStarted", "queueTime": "2026-10-28T09:00:00Z",
                "startTime": "0001-01-01T00:00:00Z", "finishTime": "0001-01-01T00:00:00Z"}"#,
        );
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());

        assert_eq!(mapped.started_at.to_rfc3339(), "2026-10-28T09:00:00+00:00");
        assert_eq!(mapped.concluded_at, None);
        assert_eq!(mapped.duration_seconds, None);
    }

    #[test]
    fn unparsable_variable_parameters_are_ignored() {
        let run = build(r#"{"id": 7, "sourceBranch": "refs/heads/dev", "parameters": "oops"}"#);
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());
        assert_eq!(mapped.inputs, Some(json!({"source-branch": "dev"})));
    }

    #[test]
    fn a_build_variable_named_branch_does_not_overwrite_the_saved_branch() {
        let run = build(
            r#"{"id": 8, "sourceBranch": "refs/heads/main", "parameters": "{\"branch\":\"legacy\"}"}"#,
        );
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());
        assert_eq!(
            mapped.inputs,
            Some(json!({"source-branch": "main", "branch": "legacy"}))
        );
    }

    #[test]
    fn project_becomes_a_selectable_item_of_the_organization() {
        let project = types::Project {
            name: "Web App".to_string(),
            description: Some(String::new()),
        };
        let available = map_available_pipeline(&project, "acme");
        assert_eq!(available.id, "Web App");
        assert_eq!(available.organization.as_deref(), Some("acme"));
        assert_eq!(available.description, None);
    }

    fn overridable(names: &[&str]) -> HashSet<String> {
        names.iter().map(|name| name.to_lowercase()).collect()
    }

    #[test]
    fn trigger_body_carries_branch_ref_and_variable_overrides() {
        let body = trigger_body(
            12,
            Some(
                json!({"source-branch": "feature/x", "deployTarget": "staging", "retries": 3, "fast": true}),
            ),
            &overridable(&["deployTarget", "retries", "fast"]),
        );

        assert_eq!(body["definition"], json!({"id": 12}));
        assert_eq!(body["sourceBranch"], "refs/heads/feature/x");
        let parameters: Value = serde_json::from_str(body["parameters"].as_str().unwrap()).unwrap();
        assert_eq!(
            parameters,
            json!({"deployTarget": "staging", "retries": "3", "fast": "true"})
        );
    }

    #[test]
    fn trigger_body_keeps_full_refs_and_drops_blank_inputs() {
        let none = overridable(&["empty", "gone"]);
        let body = trigger_body(
            12,
            Some(json!({"source-branch": "refs/tags/v1", "empty": "  ", "gone": null})),
            &none,
        );
        assert_eq!(body["sourceBranch"], "refs/tags/v1");
        assert!(body.get("parameters").is_none());

        assert_eq!(
            trigger_body(12, None, &none),
            json!({"definition": {"id": 12}})
        );
        assert_eq!(
            trigger_body(12, Some(json!({"source-branch": ""})), &none),
            json!({"definition": {"id": 12}})
        );
    }

    #[test]
    fn trigger_body_keeps_a_variable_named_branch_apart_from_the_source_branch() {
        let body = trigger_body(
            12,
            Some(json!({"source-branch": "main", "branch": "legacy"})),
            &overridable(&["branch"]),
        );
        assert_eq!(body["sourceBranch"], "refs/heads/main");
        assert_eq!(body["parameters"], r#"{"branch":"legacy"}"#);
    }

    #[test]
    fn trigger_body_sends_an_overridable_ref_even_when_it_equals_the_branch() {
        let body = trigger_body(
            12,
            Some(json!({"source-branch": "main", "ref": "main"})),
            &overridable(&["Ref"]),
        );
        assert_eq!(body["sourceBranch"], "refs/heads/main");
        assert_eq!(body["parameters"], r#"{"ref":"main"}"#);
    }

    #[test]
    fn trigger_body_drops_the_rerun_ref_echo_and_any_input_that_is_not_an_overridable_variable() {
        let body = trigger_body(
            12,
            Some(json!({"source-branch": "main", "ref": "main", "unknown": "x", "fixed": "y"})),
            &overridable(&["deployTarget"]),
        );
        assert_eq!(body["sourceBranch"], "refs/heads/main");
        assert!(body.get("parameters").is_none());
    }

    const RECORD_RESULTS: [&str; 6] = [
        "succeeded",
        "succeededWithIssues",
        "failed",
        "canceled",
        "skipped",
        "abandoned",
    ];

    fn timeline(json: &str) -> types::Timeline {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn completed_records_map_by_each_documented_result() {
        let cases = [
            ("succeeded", PipelineStatus::Success),
            ("succeededWithIssues", PipelineStatus::Failed),
            ("failed", PipelineStatus::Failed),
            ("canceled", PipelineStatus::Cancelled),
            ("skipped", PipelineStatus::Skipped),
            ("abandoned", PipelineStatus::Cancelled),
        ];
        for (result, expected) in cases {
            assert_eq!(
                map_status(Some("completed"), Some(result)),
                expected,
                "completed/{result}"
            );
        }
        assert_eq!(
            sorted(cases.iter().map(|(result, _)| *result)),
            sorted(RECORD_RESULTS)
        );
    }

    #[test]
    fn unfinished_records_map_by_state() {
        assert_eq!(map_status(Some("pending"), None), PipelineStatus::Pending);
        assert_eq!(
            map_status(Some("inProgress"), None),
            PipelineStatus::Running
        );
    }

    #[test]
    fn jobs_show_a_log_by_state_and_by_whether_the_timeline_lists_one() {
        let nodes = timeline_to_log_nodes(&timeline(
            r#"{"records": [
                {"id": "j1", "type": "Job", "name": "done", "state": "completed", "result": "succeeded", "startTime": "2026-10-28T10:00:00Z", "log": {"id": 1}},
                {"id": "j2", "type": "Job", "name": "live", "state": "inProgress", "startTime": "2026-10-28T10:00:00Z", "log": {"id": 2}},
                {"id": "j3", "type": "Job", "name": "live without a log yet", "state": "inProgress", "startTime": "2026-10-28T10:00:00Z"},
                {"id": "j4", "type": "Job", "name": "queued", "state": "pending"},
                {"id": "j5", "type": "Job", "name": "skipped", "state": "completed", "result": "skipped"},
                {"id": "j6", "type": "Job", "name": "canceled in the queue", "state": "completed", "result": "canceled"},
                {"id": "j7", "type": "Job", "name": "canceled mid-run", "state": "completed", "result": "canceled", "startTime": "2026-10-28T10:00:00Z", "log": {"id": 7}}
            ]}"#,
        ));

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| (node.name.as_str(), node.status, node.log))
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "done",
                    PipelineStatus::Success,
                    Some(LogAvailability::Ready)
                ),
                (
                    "live",
                    PipelineStatus::Running,
                    Some(LogAvailability::Partial)
                ),
                (
                    "live without a log yet",
                    PipelineStatus::Running,
                    Some(LogAvailability::Pending)
                ),
                (
                    "queued",
                    PipelineStatus::Pending,
                    Some(LogAvailability::Pending)
                ),
                ("skipped", PipelineStatus::Skipped, None),
                ("canceled in the queue", PipelineStatus::Cancelled, None),
                (
                    "canceled mid-run",
                    PipelineStatus::Cancelled,
                    Some(LogAvailability::Ready)
                ),
            ]
        );
    }

    #[test]
    fn siblings_sort_by_order_and_records_without_one_go_last() {
        let nodes = timeline_to_log_nodes(&timeline(
            r#"{"records": [
                {"id": "t3", "parentId": "j", "type": "Task", "name": "unordered"},
                {"id": "t2", "parentId": "j", "type": "Task", "name": "second", "order": 2},
                {"id": "j", "type": "Job", "name": "job", "order": 1},
                {"id": "t1", "parentId": "j", "type": "Task", "name": "first", "order": 1}
            ]}"#,
        ));

        let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["job", "first", "second", "unordered"]);
    }

    #[test]
    fn records_of_other_types_and_records_without_a_type_are_left_out() {
        let nodes = timeline_to_log_nodes(&timeline(
            r#"{"records": [
                {"id": "s", "type": "Stage", "name": "stage"},
                {"id": "c", "parentId": "s", "type": "Checkpoint", "name": "checkpoint"},
                {"id": "a", "parentId": "s", "type": "Checkpoint.Approval", "name": "approval"},
                {"id": "x", "parentId": "s"},
                {"id": "orphan-task", "parentId": "missing", "type": "Task", "name": "orphan"}
            ]}"#,
        ));

        let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["stage"]);
        assert!(timeline_to_log_nodes(&timeline("{}")).is_empty());
    }

    #[test]
    fn only_a_job_record_with_a_log_gives_a_log_id() {
        let timeline = timeline(
            r#"{"records": [
                {"id": "job", "type": "Job", "log": {"id": 7}},
                {"id": "bare-job", "type": "Job"},
                {"id": "task", "parentId": "job", "type": "Task", "log": {"id": 8}},
                {"id": "stage", "type": "Stage", "log": {"id": 9}}
            ]}"#,
        );

        assert_eq!(job_log_id(&timeline, "job"), Some(7));
        for record_id in ["bare-job", "task", "stage", "missing", "7"] {
            assert_eq!(job_log_id(&timeline, record_id), None, "{record_id}");
        }
    }
}
