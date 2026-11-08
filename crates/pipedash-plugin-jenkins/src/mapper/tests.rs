use super::*;

#[test]
fn test_map_jenkins_result() {
    assert_eq!(map_jenkins_result(Some("SUCCESS")), PipelineStatus::Success);
    assert_eq!(map_jenkins_result(Some("FAILURE")), PipelineStatus::Failed);
    assert_eq!(
        map_jenkins_result(Some("ABORTED")),
        PipelineStatus::Cancelled
    );
    assert_eq!(map_jenkins_result(None), PipelineStatus::Running);
}

fn log_node(json: &str, job_path: &str) -> LogNode {
    build_to_log_node(&serde_json::from_str(json).unwrap(), job_path)
}

#[test]
fn every_build_result_maps_to_a_status_and_log_availability() {
    let cases = [
        (
            r#""result":"SUCCESS","building":false"#,
            PipelineStatus::Success,
            LogAvailability::Ready,
        ),
        (
            r#""result":"FAILURE","building":false"#,
            PipelineStatus::Failed,
            LogAvailability::Ready,
        ),
        (
            r#""result":"UNSTABLE","building":false"#,
            PipelineStatus::Failed,
            LogAvailability::Ready,
        ),
        (
            r#""result":"ABORTED","building":false"#,
            PipelineStatus::Cancelled,
            LogAvailability::Ready,
        ),
        (
            r#""result":"NOT_BUILT","building":false"#,
            PipelineStatus::Skipped,
            LogAvailability::Ready,
        ),
        (
            r#""result":null,"building":true"#,
            PipelineStatus::Running,
            LogAvailability::Partial,
        ),
        (
            r#""result":"SUCCESS","building":true"#,
            PipelineStatus::Running,
            LogAvailability::Partial,
        ),
        (
            r#""result":"SOMETHING_NEW","building":false"#,
            PipelineStatus::Pending,
            LogAvailability::Pending,
        ),
    ];
    for (fields, status, log) in cases {
        let node = log_node(&format!(r#"{{"number":3,{fields}}}"#), "app");
        assert_eq!(node.status, status, "{fields}");
        assert_eq!(node.log, Some(log), "{fields}");
        assert_eq!(node.id, "3");
        assert_eq!(node.kind, LogNodeKind::Job);
    }
}

#[test]
fn log_node_is_named_after_the_build_or_the_job() {
    let full = r#"{"number":3,"fullDisplayName":"team » app #3"}"#;
    let blank = r#"{"number":3,"fullDisplayName":"  "}"#;
    let absent = r#"{"number":3}"#;

    assert_eq!(log_node(full, "team/app").name, "team » app #3");
    assert_eq!(log_node(blank, "team/app").name, "app #3");
    assert_eq!(log_node(absent, "app").name, "app #3");
}

#[test]
fn build_without_a_start_time_has_no_times() {
    let node = log_node(r#"{"number":3,"result":"SUCCESS","duration":5000}"#, "app");

    assert_eq!(node.started_at, None);
    assert_eq!(node.finished_at, None);
}
