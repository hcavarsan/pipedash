use serde_json::json;

use super::*;

fn pipeline(value: serde_json::Value) -> types::Pipeline {
    serde_json::from_value(value).unwrap()
}

#[test]
fn every_woodpecker_status_maps_to_a_pipedash_status() {
    for (status, expected) in [
        ("success", PipelineStatus::Success),
        ("failure", PipelineStatus::Failed),
        ("error", PipelineStatus::Failed),
        ("killed", PipelineStatus::Cancelled),
        ("canceled", PipelineStatus::Cancelled),
        ("declined", PipelineStatus::Cancelled),
        ("skipped", PipelineStatus::Skipped),
        ("running", PipelineStatus::Running),
        ("pending", PipelineStatus::Pending),
        ("blocked", PipelineStatus::Pending),
        ("created", PipelineStatus::Pending),
        ("unknown", PipelineStatus::Pending),
    ] {
        assert_eq!(map_status(status), expected, "{status}");
    }
}

#[test]
fn unset_timestamps_fall_back_to_created_and_leave_duration_empty() {
    let run = pipeline(json!({
        "id": 9,
        "number": 4,
        "status": "pending",
        "created": 1_761_645_600,
        "started": 0,
        "finished": 0,
        "message": "  fix build  \nbody",
    }));
    let mapped = map_run(&run, "5", String::new());
    assert_eq!(mapped.started_at.timestamp(), 1_761_645_600);
    assert_eq!(mapped.concluded_at, None);
    assert_eq!(mapped.duration_seconds, None);
    assert_eq!(mapped.commit_message.as_deref(), Some("fix build"));
    assert_eq!(mapped.branch, None);
    assert_eq!(mapped.inputs, None);
}

#[test]
fn steps_nest_under_their_workflow_with_log_availability() {
    let run = pipeline(json!({
        "id": 9,
        "number": 4,
        "status": "running",
        "workflows": [{
            "id": 30,
            "name": "build",
            "state": "running",
            "children": [
                {"id": 1, "pid": 2, "name": "clone", "state": "success", "started": 10},
                {"id": 2, "pid": 3, "name": "test", "state": "running", "started": 11},
                {"id": 3, "pid": 4, "name": "lint", "state": "pending"},
                {"id": 4, "pid": 5, "name": "deploy", "state": "skipped"},
                {"id": 5, "pid": 6, "name": "notify", "state": "success", "started": 0}
            ]
        }]
    }));
    let nodes = workflows_to_log_nodes(&run, "http://ci/repos/5/pipeline/4");
    let summary: Vec<_> = nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.parent_id.as_deref(),
                node.kind,
                node.log,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("workflow:30", None, LogNodeKind::Stage, None),
            (
                "1",
                Some("workflow:30"),
                LogNodeKind::Job,
                Some(LogAvailability::Ready)
            ),
            (
                "2",
                Some("workflow:30"),
                LogNodeKind::Job,
                Some(LogAvailability::Partial)
            ),
            (
                "3",
                Some("workflow:30"),
                LogNodeKind::Job,
                Some(LogAvailability::Pending)
            ),
            ("4", Some("workflow:30"), LogNodeKind::Job, None),
            ("5", Some("workflow:30"), LogNodeKind::Job, None),
        ]
    );
    assert_eq!(
        nodes[1].web_url.as_deref(),
        Some("http://ci/repos/5/pipeline/4/2")
    );
}
