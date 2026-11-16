use super::*;

#[test]
fn test_map_build_state() {
    assert_eq!(map_build_state("passed"), PipelineStatus::Success);
    assert_eq!(map_build_state("failed"), PipelineStatus::Failed);
    assert_eq!(map_build_state("running"), PipelineStatus::Running);
    assert_eq!(map_build_state("canceled"), PipelineStatus::Cancelled);
    assert_eq!(map_build_state("skipped"), PipelineStatus::Skipped);
    assert_eq!(map_build_state("unknown"), PipelineStatus::Pending);
}

#[test]
fn every_job_state_maps_to_a_status() {
    let cases = [
        ("pending", PipelineStatus::Pending),
        ("waiting", PipelineStatus::Pending),
        ("blocked", PipelineStatus::Pending),
        ("unblocked", PipelineStatus::Pending),
        ("limiting", PipelineStatus::Pending),
        ("limited", PipelineStatus::Pending),
        ("scheduled", PipelineStatus::Pending),
        ("assigned", PipelineStatus::Pending),
        ("accepted", PipelineStatus::Pending),
        ("running", PipelineStatus::Running),
        ("canceling", PipelineStatus::Running),
        ("timing_out", PipelineStatus::Running),
        ("passed", PipelineStatus::Success),
        ("failed", PipelineStatus::Failed),
        ("timed_out", PipelineStatus::Failed),
        ("canceled", PipelineStatus::Cancelled),
        ("expired", PipelineStatus::Cancelled),
        ("skipped", PipelineStatus::Skipped),
        ("broken", PipelineStatus::Skipped),
        ("waiting_failed", PipelineStatus::Skipped),
        ("blocked_failed", PipelineStatus::Skipped),
        ("unblocked_failed", PipelineStatus::Skipped),
    ];
    for (state, status) in cases {
        assert_eq!(map_job_state(state), status, "{state}");
    }
}

#[test]
fn script_jobs_of_the_latest_attempt_become_log_nodes() {
    let build: types::BuildJobs = serde_json::from_str(
        r#"{"number":2,"state":"failed","jobs":[
            {"id":"old","type":"script","name":"RSpec","state":"failed","retried":true,
             "started_at":"2026-10-27T10:00:00.000Z","finished_at":"2026-10-27T10:01:00.000Z"},
            {"id":"new","type":"script","name":"RSpec","state":"passed","retried":false,
             "web_url":"https://buildkite.com/acme/app/builds/2#new",
             "started_at":"2026-10-27T10:02:00.000Z","finished_at":"2026-10-27T10:03:30.000Z"},
            {"id":"wait","type":"waiter","state":null},
            {"id":"gate","type":"manual","label":"Deploy?","state":"blocked"},
            {"id":"shard","type":"script","name":":rspec: Tests","state":"running",
             "started_at":"2026-10-27T10:04:00.000Z","parallel_group_index":1,"parallel_group_total":4},
            {"id":"queued","type":"script","name":null,"command":"\nmake lint\nmake test","state":"scheduled"},
            {"id":"never","type":"script","step_key":"deploy","state":"broken"},
            {"id":"stopped","type":"script","name":"Lint","state":"canceled",
             "started_at":"2026-10-27T10:05:00.000Z","finished_at":"2026-10-27T10:05:10.000Z"}
        ]}"#,
    )
    .unwrap();

    let nodes = jobs_to_log_nodes(&build.jobs);

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.name.as_str(), node.status, node.log))
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                "new",
                "RSpec",
                PipelineStatus::Success,
                Some(LogAvailability::Ready)
            ),
            (
                "shard",
                ":rspec: Tests (2/4)",
                PipelineStatus::Running,
                Some(LogAvailability::Partial)
            ),
            (
                "queued",
                "make lint",
                PipelineStatus::Pending,
                Some(LogAvailability::Pending)
            ),
            ("never", "deploy", PipelineStatus::Skipped, None),
            (
                "stopped",
                "Lint",
                PipelineStatus::Cancelled,
                Some(LogAvailability::Ready)
            ),
        ]
    );
    assert!(nodes
        .iter()
        .all(|node| node.kind == LogNodeKind::Job && node.parent_id.is_none()));
    let new = &nodes[0];
    assert_eq!(
        new.web_url.as_deref(),
        Some("https://buildkite.com/acme/app/builds/2#new")
    );
    assert_eq!(
        new.finished_at.unwrap() - new.started_at.unwrap(),
        chrono::Duration::seconds(90)
    );
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn shop() -> RepoId {
    RepoId::from_host_path("github.com", "acme/shop").unwrap()
}

fn observed(commit: Option<&str>) -> Correlation {
    Correlation::single_source(Some(shop()), commit.and_then(CommitSha::parse))
}

fn build_json(commit: &str, repository: &str) -> types::Build {
    serde_json::from_str(&format!(
        r#"{{"id": "b1", "number": 7, "state": "passed", "web_url": "https://x/7",
            "commit": "{commit}", "created_at": "2026-10-28T10:00:00Z",
            "pipeline": {repository}}}"#
    ))
    .unwrap()
}

#[test]
fn run_observes_the_build_repository_and_commit() {
    let build = || build_json(SHA, r#"{"repository": "git@github.com:acme/shop.git"}"#);

    let run = build_to_pipeline_run(build(), "acme__shop");

    assert_eq!(run.correlation, observed(Some(SHA)));
    assert_eq!(run.evidence, RunEvidence::default());
    assert_eq!(
        build_to_pipeline_run(build(), "acme__shop").correlation,
        run.correlation
    );
}

#[test]
fn a_symbolic_commit_keeps_the_repository_without_a_commit() {
    let run = build_to_pipeline_run(
        build_json("HEAD", r#"{"repository": "https://github.com/acme/shop"}"#),
        "acme__shop",
    );
    assert_eq!(run.correlation, observed(None));
}

#[test]
fn a_build_without_a_pipeline_repository_observes_no_source() {
    for repository in ["{}", r#"{"repository": "not a url"}"#, "null"] {
        let run = build_to_pipeline_run(build_json(SHA, repository), "acme__shop");
        assert!(run.correlation.is_empty(), "{repository}");
    }
}

#[test]
fn pipeline_observes_the_repository_of_its_definition() {
    let pipeline = |repository: &str| types::Pipeline {
        name: "Shop".to_string(),
        repository: repository.to_string(),
    };
    let latest = build_json(SHA, "null");

    let mapped = map_pipeline(
        "acme",
        "shop",
        pipeline("git@github.com:acme/shop.git"),
        Some(&latest),
    );

    assert_eq!(mapped.id, "acme__shop");
    assert_eq!(mapped.status, PipelineStatus::Success);
    assert_eq!(mapped.metadata["organization_slug"], "acme");
    assert_eq!(mapped.correlation, observed(None));
    assert!(map_pipeline("acme", "shop", pipeline("local-path"), None)
        .correlation
        .is_empty());
}
