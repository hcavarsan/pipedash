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

    let pipeline = map_pipeline("acme", "Web App", &definition);

    assert_eq!(pipeline.id, "Web App/12");
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
    let pipeline = map_pipeline("acme", "App", &definition);
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

    let mapped = map_run(&run, "App/12", "App", &server());

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
    let mapped = map_run(&run, "App/12", "App", &server());
    assert_eq!(mapped.branch.as_deref(), Some("refs/pull/17/merge"));
}

#[test]
fn run_with_only_an_id_still_maps() {
    let run = build(r#"{"id": 5}"#);
    let mapped = map_run(&run, "My App/12", "My App", &server());

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
    let mapped = map_run(&run, "App/12", "App", &server());

    assert_eq!(mapped.started_at.to_rfc3339(), "2026-10-28T09:00:00+00:00");
    assert_eq!(mapped.concluded_at, None);
    assert_eq!(mapped.duration_seconds, None);
}

#[test]
fn unparsable_variable_parameters_are_ignored() {
    let run = build(r#"{"id": 7, "sourceBranch": "refs/heads/dev", "parameters": "oops"}"#);
    let mapped = map_run(&run, "App/12", "App", &server());
    assert_eq!(mapped.inputs, Some(json!({"source-branch": "dev"})));
}

#[test]
fn a_build_variable_named_branch_does_not_overwrite_the_saved_branch() {
    let run = build(
        r#"{"id": 8, "sourceBranch": "refs/heads/main", "parameters": "{\"branch\":\"legacy\"}"}"#,
    );
    let mapped = map_run(&run, "App/12", "App", &server());
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
fn only_a_job_record_with_a_log_gives_a_log_id_and_its_attempt() {
    let timeline = timeline(
        r#"{"records": [
            {"id": "job", "type": "Job", "attempt": 2, "log": {"id": 7}},
            {"id": "first-try", "type": "Job", "log": {"id": 10}},
            {"id": "bare-job", "type": "Job"},
            {"id": "task", "parentId": "job", "type": "Task", "log": {"id": 8}},
            {"id": "stage", "type": "Stage", "log": {"id": 9}}
        ]}"#,
    );

    assert_eq!(job_log_ref(&timeline, "job"), Some((7, 2)));
    assert_eq!(job_log_ref(&timeline, "first-try"), Some((10, 0)));
    for record_id in ["bare-job", "task", "stage", "missing", "7"] {
        assert_eq!(job_log_ref(&timeline, record_id), None, "{record_id}");
    }
}
