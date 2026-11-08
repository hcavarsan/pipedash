use super::*;

fn state(json: serde_json::Value) -> types::PipelineState {
    serde_json::from_value(json).unwrap()
}

#[test]
fn every_pipeline_and_step_state_maps_to_a_status() {
    let completed =
        |result: &str| serde_json::json!({"name": "COMPLETED", "result": {"name": result}});
    let cases = [
        (
            serde_json::json!({"name": "PENDING"}),
            PipelineStatus::Pending,
        ),
        (
            serde_json::json!({"name": "READY"}),
            PipelineStatus::Pending,
        ),
        (
            serde_json::json!({"name": "PARSING"}),
            PipelineStatus::Running,
        ),
        (
            serde_json::json!({"name": "IN_PROGRESS"}),
            PipelineStatus::Running,
        ),
        (
            serde_json::json!({"name": "IN_PROGRESS", "stage": {"name": "PAUSED"}}),
            PipelineStatus::Pending,
        ),
        (
            serde_json::json!({"name": "PAUSED"}),
            PipelineStatus::Pending,
        ),
        (
            serde_json::json!({"name": "HALTED"}),
            PipelineStatus::Pending,
        ),
        (completed("SUCCESSFUL"), PipelineStatus::Success),
        (completed("FAILED"), PipelineStatus::Failed),
        (completed("ERROR"), PipelineStatus::Failed),
        (completed("STOPPED"), PipelineStatus::Cancelled),
        (completed("EXPIRED"), PipelineStatus::Cancelled),
        (completed("NOT_RUN"), PipelineStatus::Skipped),
        (
            serde_json::json!({"name": "COMPLETED"}),
            PipelineStatus::Failed,
        ),
        (
            serde_json::json!({"name": "SOMETHING_NEW"}),
            PipelineStatus::Pending,
        ),
    ];
    for (json, status) in cases {
        assert_eq!(map_status(&state(json.clone())), status, "{json}");
    }
}

#[test]
fn steps_become_job_nodes_with_the_availability_of_their_log() {
    let steps: Vec<types::PipelineStep> = serde_json::from_str(
        r#"[
            {"uuid":"{a}","name":"Build","state":{"name":"COMPLETED","result":{"name":"SUCCESSFUL"}},
             "started_on":"2026-10-28T10:00:00.000Z","completed_on":"2026-10-28T10:01:15.000Z"},
            {"uuid":"{b}","name":" Test ","state":{"name":"COMPLETED","result":{"name":"FAILED"}},
             "started_on":"2026-10-28T10:01:16.000Z","completed_on":"2026-10-28T10:02:00.000Z"},
            {"uuid":"{c}","name":"Deploy","state":{"name":"IN_PROGRESS"},
             "started_on":"2026-10-28T10:02:01.000Z"},
            {"uuid":"{d}","name":"Approve","state":{"name":"PENDING","stage":{"name":"PAUSED"}}},
            {"uuid":"{e}","name":null,"state":{"name":"READY"}},
            {"uuid":"{f}","name":"Smoke","state":{"name":"COMPLETED","result":{"name":"NOT_RUN"}}},
            {"uuid":"{g}","name":"Release","state":{"name":"COMPLETED","result":{"name":"STOPPED"}},
             "started_on":"2026-10-28T10:03:00.000Z","completed_on":"2026-10-28T10:03:10.000Z"},
            {"uuid":"{h}","name":"Cleanup","state":{"name":"COMPLETED","result":{"name":"STOPPED"}}}
        ]"#,
    )
    .unwrap();

    let nodes = steps_to_log_nodes(&steps);

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.name.as_str(), node.status, node.log))
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                "{a}",
                "Build",
                PipelineStatus::Success,
                Some(LogAvailability::Ready)
            ),
            (
                "{b}",
                "Test",
                PipelineStatus::Failed,
                Some(LogAvailability::Ready)
            ),
            (
                "{c}",
                "Deploy",
                PipelineStatus::Running,
                Some(LogAvailability::Partial)
            ),
            (
                "{d}",
                "Approve",
                PipelineStatus::Pending,
                Some(LogAvailability::Pending)
            ),
            (
                "{e}",
                "Step 5",
                PipelineStatus::Pending,
                Some(LogAvailability::Pending)
            ),
            ("{f}", "Smoke", PipelineStatus::Skipped, None),
            (
                "{g}",
                "Release",
                PipelineStatus::Cancelled,
                Some(LogAvailability::Ready)
            ),
            ("{h}", "Cleanup", PipelineStatus::Cancelled, None),
        ]
    );
    assert!(nodes
        .iter()
        .all(|node| node.kind == LogNodeKind::Job && node.parent_id.is_none()));
    assert_eq!(
        nodes[0].finished_at.unwrap() - nodes[0].started_at.unwrap(),
        chrono::Duration::seconds(75)
    );
    assert!(nodes[2].started_at.is_some() && nodes[2].finished_at.is_none());
    assert!(nodes[3].started_at.is_none());
}
