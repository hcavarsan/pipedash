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
