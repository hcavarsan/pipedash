use pipedash_plugin_api::{
    ClusterId,
    CommitSha,
    Correlation,
    RepoId,
    TargetLifetime,
    TargetObservation,
    UnitId,
};

use super::*;

fn stack(json: &str) -> StackListItem {
    serde_json::from_str(json).unwrap()
}

fn update_item(json: &str) -> UpdateListItem {
    serde_json::from_str(json).unwrap()
}

const MAIN_STACK: &str = r#"{
    "id": "66a1", "type": "Stack", "name": "web", "template": false, "tags": [],
    "info": {
        "swarm_id": "", "swarm_name": "", "server_id": "66s1", "server_name": "prod-1",
        "files_on_host": false, "file_contents": false, "linked_repo": "",
        "linked_repo_name": "", "git_provider": "github.com", "repo": "acme/web",
        "branch": "main", "repo_link": "https://github.com/acme/web/tree/main",
        "state": "running", "status": "running(2)",
        "services": [
            {"service": "app", "image": "acme/web:1", "latest_image": "acme/web:2", "update_available": true},
            {"service": "db", "image": "postgres:16", "latest_image": null, "update_available": false}
        ],
        "auto_update_all_services": false, "project_missing": false, "missing_files": [],
        "deployed_hash": "abc1234", "latest_hash": "def5678"
    }
}"#;

const V1_STACK: &str = r#"{
    "id": "66a2", "type": "Stack", "name": "files", "template": false, "tags": [],
    "info": {
        "server_id": "66s1", "files_on_host": true, "file_contents": false,
        "linked_repo": "", "git_provider": "github.com", "repo": "", "branch": "main",
        "repo_link": "", "state": "down", "status": null,
        "services": [{"service": "app", "image": "nginx", "update_available": false}],
        "project_missing": false, "missing_files": ["compose.yaml"],
        "deployed_hash": null, "latest_hash": null
    }
}"#;

fn info(json: Value) -> StackInfo {
    serde_json::from_value(json).unwrap()
}

#[test]
fn stack_states_become_pipeline_statuses_and_labels() {
    let expected = [
        ("running", PipelineStatus::Success, "Up"),
        ("deploying", PipelineStatus::Running, "Deploying"),
        ("removing", PipelineStatus::Running, "Removing"),
        ("restarting", PipelineStatus::Degraded, "Restarting"),
        ("unhealthy", PipelineStatus::Degraded, "Unhealthy"),
        ("dead", PipelineStatus::Failed, "Dead"),
        ("paused", PipelineStatus::Cancelled, "Paused"),
        ("stopped", PipelineStatus::Cancelled, "Stopped"),
        ("down", PipelineStatus::Cancelled, "Down"),
        ("created", PipelineStatus::Pending, "Created"),
        ("unknown", PipelineStatus::Pending, "Unknown"),
        ("something_new", PipelineStatus::Pending, "Unknown"),
        ("", PipelineStatus::Pending, "Unknown"),
    ];
    for (state, status, label) in expected {
        let (actual, resource_state) = map_stack_state(&info(json!({"state": state})));
        assert_eq!(actual, status, "{state}");
        assert_eq!(resource_state.label, label, "{state}");
        assert_eq!(resource_state.message, None, "{state}");
    }
}

#[test]
fn unhealthy_state_is_explained_by_docker_status_and_missing_files() {
    let (status, resource_state) = map_stack_state(&info(json!({
        "state": "unhealthy", "status": "unhealthy(1)",
        "project_missing": true, "missing_files": ["compose.yaml", ".env"]
    })));

    assert_eq!(status, PipelineStatus::Degraded);
    assert_eq!(
        resource_state,
        ResourceState::new(
            "Unhealthy",
            Some("Project is missing on the host; Missing files: compose.yaml, .env; unhealthy(1)")
        )
    );
}

#[test]
fn restarting_state_carries_the_docker_status() {
    let (status, resource_state) = map_stack_state(&info(
        json!({"state": "restarting", "status": "restarting(1)"}),
    ));

    assert_eq!(status, PipelineStatus::Degraded);
    assert_eq!(
        resource_state,
        ResourceState::new("Restarting", Some("restarting(1)"))
    );
}

#[test]
fn running_state_has_no_message_even_with_a_docker_status() {
    let (_, resource_state) = map_stack_state(&info(json!({
        "state": "running", "status": "running(2)", "missing_files": ["extra.yaml"]
    })));

    assert_eq!(resource_state, ResourceState::new("Up", None));
}

#[test]
fn blank_docker_status_adds_no_message() {
    let (_, resource_state) = map_stack_state(&info(json!({"state": "stopped", "status": ""})));

    assert_eq!(resource_state.message, None);
}

#[test]
fn update_statuses_and_success_become_run_statuses() {
    let expected = [
        ("Queued", false, PipelineStatus::Pending),
        ("Queued", true, PipelineStatus::Pending),
        ("InProgress", false, PipelineStatus::Running),
        ("InProgress", true, PipelineStatus::Running),
        ("Complete", true, PipelineStatus::Success),
        ("Complete", false, PipelineStatus::Failed),
        ("SomethingNew", true, PipelineStatus::Pending),
        ("", false, PipelineStatus::Pending),
    ];
    for (status, success, expected) in expected {
        assert_eq!(
            map_update_status(status, success),
            expected,
            "{status} {success}"
        );
    }
}

#[test]
fn stack_on_main_maps_server_repo_and_update_facts() {
    let pipeline = map_pipeline(&stack(MAIN_STACK), Some(1761660000123));

    assert_eq!(pipeline.id, "66a1");
    assert_eq!(pipeline.name, "web");
    assert_eq!(pipeline.status, PipelineStatus::Success);
    assert_eq!(pipeline.repository, "prod-1/web");
    assert_eq!(pipeline.branch.as_deref(), Some("main"));
    assert_eq!(
        pipeline.last_run.map(|at| at.timestamp_millis()),
        Some(1761660000123)
    );
    assert_eq!(pipeline.metadata["server"], json!("prod-1"));
    assert!(!pipeline.metadata.contains_key("state"));
    assert_eq!(
        pipeline.resource_state,
        Some(ResourceState::new("Up", None))
    );
    assert_eq!(pipeline.metadata["update_available"], json!(true));
    assert_eq!(pipeline.metadata["service_count"], json!(2));
    assert_eq!(pipeline.metadata["repo"], json!("acme/web"));
    assert_eq!(
        pipeline.metadata["repo_url"],
        json!("https://github.com/acme/web/tree/main")
    );
    assert_eq!(pipeline.metadata["docker_status"], json!("running(2)"));
    assert_eq!(pipeline.metadata["deployed_hash"], json!("abc1234"));
    assert_eq!(pipeline.metadata["latest_hash"], json!("def5678"));
}

#[test]
fn stack_on_v1_without_names_falls_back_to_server_id_and_skips_empty_facts() {
    let pipeline = map_pipeline(&stack(V1_STACK), None);

    assert_eq!(pipeline.status, PipelineStatus::Cancelled);
    assert_eq!(
        pipeline.resource_state,
        Some(ResourceState::new(
            "Down",
            Some("Missing files: compose.yaml")
        ))
    );
    assert_eq!(pipeline.last_run, None);
    assert_eq!(pipeline.repository, "66s1/files");
    assert_eq!(pipeline.branch, None);
    assert_eq!(pipeline.metadata["server"], json!("66s1"));
    assert_eq!(pipeline.metadata["update_available"], json!(false));
    assert_eq!(pipeline.metadata["missing_files"], json!(1));
    for key in [
        "repo",
        "repo_url",
        "docker_status",
        "deployed_hash",
        "latest_hash",
    ] {
        assert!(!pipeline.metadata.contains_key(key), "{key}");
    }
}

#[test]
fn stack_with_only_a_name_is_still_mapped() {
    let pipeline = map_pipeline(&stack(r#"{"id": "x", "name": "bare"}"#), None);

    assert_eq!(pipeline.repository, "bare");
    assert_eq!(pipeline.status, PipelineStatus::Pending);
    assert!(!pipeline.metadata.contains_key("server"));
}

#[test]
fn swarm_stack_is_grouped_by_swarm_name() {
    let pipeline = map_pipeline(
        &stack(
            r#"{"id": "x", "name": "api", "info": {"swarm_id": "s", "swarm_name": "edge", "server_id": "", "state": "running"}}"#,
        ),
        None,
    );

    assert_eq!(pipeline.repository, "edge/api");
    assert_eq!(pipeline.metadata["server"], json!("edge"));
}

#[test]
fn available_pipeline_is_keyed_by_stack_id() {
    let available = map_available_pipeline(&stack(MAIN_STACK));

    assert_eq!(available.id, "66a1");
    assert_eq!(available.name, "web");
    assert_eq!(available.organization.as_deref(), Some("prod-1"));
    assert_eq!(available.repository.as_deref(), Some("acme/web"));
    assert_eq!(
        available.description.as_deref(),
        Some("State: running | Server: prod-1 | Repo: acme/web@main")
    );
}

#[test]
fn listed_update_maps_actor_status_start_and_link() {
    let item = update_item(
        r#"{"id": "66u1", "operation": "DeployStack", "start_ts": 1761660000123, "success": true,
            "username": "ada", "operator": "66usr", "target": {"type": "Stack", "id": "66a1"},
            "status": "Complete"}"#,
    );
    let run = map_run("66a1", "https://komodo.test", &item, None, None);

    assert_eq!(run.id, "66a1__66u1");
    assert_eq!(run.run_number, 1761660000123);
    assert_eq!(run.status, PipelineStatus::Success);
    assert_eq!(run.actor.as_deref(), Some("ada"));
    assert_eq!(run.started_at.timestamp_millis(), 1761660000123);
    assert_eq!(run.concluded_at, None);
    assert_eq!(run.duration_seconds, None);
    assert_eq!(run.logs_url, "https://komodo.test/updates/66u1");
    assert_eq!(run.metadata["operation"], json!("DeployStack"));
    assert_eq!(run.metadata["update_id"], json!("66u1"));
}

#[test]
fn detailed_update_adds_end_duration_commit_and_current_status() {
    let item = update_item(
        r#"{"id": "66u1", "operation": "DeployStack", "start_ts": 1761660000000,
            "success": false, "username": "", "operator": "Github", "status": "InProgress"}"#,
    );
    let detail: Update = serde_json::from_str(
        r#"{"_id": {"$oid": "66u1"}, "operation": "DeployStack", "start_ts": 1761660000000,
            "success": false, "operator": "Github", "logs": [], "end_ts": 1761660042500,
            "status": "Complete", "commit_hash": "abc1234"}"#,
    )
    .unwrap();
    let run = map_run("66a1", "https://komodo.test", &item, Some(&detail), None);

    assert_eq!(run.status, PipelineStatus::Failed);
    assert_eq!(run.actor.as_deref(), Some("Github"));
    assert_eq!(run.duration_seconds, Some(42));
    assert_eq!(
        run.concluded_at.map(|at| at.timestamp_millis()),
        Some(1761660042500)
    );
    assert_eq!(run.commit_sha.as_deref(), Some("abc1234"));
}

#[test]
fn update_without_id_links_to_the_stack_page() {
    let run = map_run(
        "66a1",
        "https://komodo.test",
        &update_item(r#"{"start_ts": 5}"#),
        None,
        None,
    );

    assert_eq!(run.logs_url, "https://komodo.test/stacks/66a1");
    assert_eq!(run.status, PipelineStatus::Pending);
    assert_eq!(run.actor, None);
}

#[test]
fn update_ids_parse_from_object_id_and_plain_string() {
    let object: Update = serde_json::from_str(r#"{"_id": {"$oid": "66u1"}}"#).unwrap();
    let plain: Update = serde_json::from_str(r#"{"_id": "66u2"}"#).unwrap();
    let missing: Update = serde_json::from_str("{}").unwrap();

    assert_eq!(object.id.unwrap().into_string(), "66u1");
    assert_eq!(plain.id.unwrap().into_string(), "66u2");
    assert!(missing.id.is_none());
}

const STAGES: &str = r#"[
    {"stage": "Clone Repo", "command": "git clone", "stdout": "ok", "stderr": "", "success": true, "start_ts": 1761660000000, "end_ts": 1761660002000},
    {"stage": "Compose Up", "command": "docker compose up -d", "stdout": "", "stderr": "boom", "success": false, "start_ts": 1761660002000, "end_ts": 1761660009500}
]"#;

fn log_nodes(status: &str, stages: &str) -> Vec<LogNode> {
    let update: UpdateLogs =
        serde_json::from_str(&format!(r#"{{"status": "{status}", "logs": {stages}}}"#)).unwrap();
    map_log_nodes(&update, "https://komodo.test/updates/66u1")
}

#[test]
fn every_update_status_maps_stages_to_a_status_and_availability() {
    let finished = [
        (PipelineStatus::Success, LogAvailability::Ready),
        (PipelineStatus::Failed, LogAvailability::Ready),
    ];
    let in_progress = [
        (PipelineStatus::Success, LogAvailability::Ready),
        (PipelineStatus::Running, LogAvailability::Partial),
    ];
    let cases = [
        ("Complete", finished),
        ("Queued", finished),
        ("something_new", finished),
        ("", finished),
        ("InProgress", in_progress),
    ];
    for (update_status, expected) in cases {
        let nodes = log_nodes(update_status, STAGES);
        let actual: Vec<_> = nodes
            .iter()
            .map(|node| (node.status, node.log.unwrap()))
            .collect();
        assert_eq!(actual, expected, "{update_status:?}");
    }
}

#[test]
fn stages_become_job_nodes_named_after_the_stage_with_their_own_times() {
    let nodes = log_nodes("Complete", STAGES);

    assert_eq!(
        nodes
            .iter()
            .map(|node| (node.id.as_str(), node.name.as_str(), node.kind))
            .collect::<Vec<_>>(),
        [
            ("0", "Clone Repo", LogNodeKind::Job),
            ("1", "Compose Up", LogNodeKind::Job)
        ]
    );
    assert!(nodes.iter().all(|node| node.parent_id.is_none()));
    assert!(nodes
        .iter()
        .all(|node| node.web_url.as_deref() == Some("https://komodo.test/updates/66u1")));
    let millis = |at: Option<DateTime<Utc>>| at.map(|at| at.timestamp_millis());
    assert_eq!(millis(nodes[0].started_at), Some(1761660000000));
    assert_eq!(millis(nodes[0].finished_at), Some(1761660002000));
    assert_eq!(millis(nodes[1].started_at), Some(1761660002000));
    assert_eq!(millis(nodes[1].finished_at), Some(1761660009500));
}

#[test]
fn the_open_stage_of_a_running_update_has_no_end_time() {
    let nodes = log_nodes("InProgress", STAGES);

    assert!(nodes[0].finished_at.is_some());
    assert!(nodes[1].started_at.is_some());
    assert_eq!(nodes[1].finished_at, None);
}

#[test]
fn unnamed_stages_and_missing_times_get_plain_fallbacks() {
    let nodes = log_nodes(
        "Complete",
        r#"[{"stage": " ", "success": true}, {"stage": "Compose Down", "success": true}]"#,
    );

    assert_eq!(nodes[0].name, "Stage 1");
    assert_eq!(nodes[1].name, "Compose Down");
    assert_eq!(nodes[0].started_at, None);
    assert_eq!(nodes[0].finished_at, None);
}

#[test]
fn an_update_without_logs_has_no_nodes() {
    assert!(log_nodes("Queued", "[]").is_empty());
    assert!(log_nodes("InProgress", "[]").is_empty());
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn target(cluster: &str, name: &str) -> TargetObservation {
    TargetObservation {
        cluster: ClusterId::from_host(cluster).unwrap(),
        namespace: None,
        name: name.to_string(),
        lifetime: TargetLifetime::Standing,
    }
}

fn github_repo() -> Option<RepoId> {
    RepoId::from_host_path("github.com", "acme/web")
}

fn detailed(commit_hash: &str) -> Update {
    serde_json::from_value(json!({
        "_id": "66u1", "start_ts": 1761660000000_i64, "end_ts": 1761660042500_i64,
        "success": true, "status": "Complete", "commit_hash": commit_hash
    }))
    .unwrap()
}

#[test]
fn a_stack_reports_its_unit_repo_without_the_branch_suffix_and_server_target() {
    let pipeline = map_pipeline(&stack(MAIN_STACK), None);

    assert_eq!(
        pipeline.correlation,
        Correlation {
            unit: UnitId::new("66a1"),
            targets: vec![target("prod-1", "prod-1")],
            ..Correlation::single_source(github_repo(), None)
        }
    );
}

#[test]
fn a_full_deployed_hash_becomes_the_source_commit_and_a_short_one_does_not() {
    let full = stack(&format!(
        r#"{{"id": "s2", "name": "api", "info": {{"server_id": "66s1", "repo": "acme/api",
            "branch": "dev", "repo_link": "https://git.example.com/acme/api",
            "state": "running", "deployed_hash": "{SHA}"}}}}"#
    ));

    assert_eq!(
        map_pipeline(&full, None).correlation,
        Correlation {
            unit: UnitId::new("s2"),
            targets: vec![target("66s1", "66s1")],
            ..Correlation::single_source(
                RepoId::from_host_path("git.example.com", "acme/api"),
                CommitSha::parse(SHA),
            )
        }
    );
}

#[test]
fn a_stack_without_a_repo_or_server_has_only_a_unit() {
    let files = map_pipeline(&stack(V1_STACK), None).correlation;
    let bare = map_pipeline(&stack(r#"{"id": "x", "name": "bare"}"#), None).correlation;

    assert_eq!(
        files,
        Correlation {
            unit: UnitId::new("66a2"),
            targets: vec![target("66s1", "66s1")],
            ..Correlation::default()
        }
    );
    assert_eq!(
        bare,
        Correlation {
            unit: UnitId::new("x"),
            ..Correlation::default()
        }
    );
}

#[test]
fn a_listed_update_has_one_timestamp_so_its_timing_is_absent() {
    let item = update_item(
        r#"{"id": "66u1", "operation": "DeployStack", "start_ts": 1761660000123,
            "success": true, "status": "Complete"}"#,
    );
    let info = stack(MAIN_STACK).info;

    let run = map_run("66a1", "", &item, None, Some(&info));

    assert_eq!(
        run.correlation,
        Correlation::single_source(github_repo(), None)
    );
    assert_eq!(run.duration_seconds, None);
    assert_eq!(
        run.evidence,
        RunEvidence {
            outcome: Observed::Recorded,
            timing: Observed::Absent,
            attempt: true,
        }
    );
}

#[test]
fn a_detailed_update_carries_the_commit_when_it_is_a_full_hash() {
    let item = update_item(r#"{"id": "66u1", "start_ts": 1761660000000}"#);
    let info = stack(MAIN_STACK).info;

    let full = map_run("66a1", "", &item, Some(&detailed(SHA)), Some(&info));
    let short = map_run("66a1", "", &item, Some(&detailed("abc1234")), Some(&info));
    let unknown = map_run("66a1", "", &item, Some(&detailed(SHA)), None);

    assert_eq!(
        full.correlation,
        Correlation::single_source(github_repo(), CommitSha::parse(SHA))
    );
    assert_eq!(full.evidence, RunEvidence::default());
    assert_eq!(
        short.correlation,
        Correlation::single_source(github_repo(), None)
    );
    assert_eq!(unknown.correlation, Correlation::default());
}

#[test]
fn a_status_taken_from_live_health_is_not_a_recorded_outcome() {
    let item = update_item(r#"{"id": "66u1", "start_ts": 1761660000000}"#);
    let mut run = map_run("66a1", "", &item, Some(&detailed(SHA)), None);

    apply_live_status(&mut run, PipelineStatus::Success);
    assert_eq!(run.status, PipelineStatus::Success);
    assert_eq!(run.evidence, RunEvidence::default());

    apply_live_status(&mut run, PipelineStatus::Failed);
    assert_eq!(run.status, PipelineStatus::Degraded);
    assert_eq!(
        run.evidence,
        RunEvidence {
            outcome: Observed::Absent,
            timing: Observed::Recorded,
            attempt: false,
        }
    );
}
