use pipedash_plugin_api::{
    LogAvailability,
    LogNodeKind,
};

use super::logs::{
    listed_taskruns,
    taskruns_to_log_nodes,
};
use super::*;
use crate::types::TektonTaskRun;

fn succeeded(status: &str, reason: &str) -> Vec<types::Condition> {
    let json = format!(r#"[{{"type":"Succeeded","status":"{status}","reason":"{reason}"}}]"#);

    serde_json::from_str(&json).unwrap()
}

#[test]
fn maps_each_succeeded_condition_to_a_status() {
    let cases = [
        ("Unknown", "Started", PipelineStatus::Running),
        ("Unknown", "Running", PipelineStatus::Running),
        ("Unknown", "PipelineRunStopping", PipelineStatus::Running),
        (
            "Unknown",
            "CancelledRunningFinally",
            PipelineStatus::Running,
        ),
        ("Unknown", "StoppedRunningFinally", PipelineStatus::Running),
        (
            "Unknown",
            "PipelineRunTimeoutRunningFinally",
            PipelineStatus::Running,
        ),
        ("Unknown", "PipelineRunPending", PipelineStatus::Pending),
        ("True", "Succeeded", PipelineStatus::Success),
        ("True", "Completed", PipelineStatus::Success),
        ("False", "Failed", PipelineStatus::Failed),
        ("False", "PipelineRunTimeout", PipelineStatus::Failed),
        ("False", "PipelineValidationFailed", PipelineStatus::Failed),
        ("False", "CouldntGetPipeline", PipelineStatus::Failed),
        ("False", "Cancelled", PipelineStatus::Cancelled),
        ("False", "PipelineRunCancelled", PipelineStatus::Cancelled),
        ("Unknown", "Pending", PipelineStatus::Pending),
        ("Unknown", "TaskRunCancelled", PipelineStatus::Running),
        ("False", "TaskRunCancelled", PipelineStatus::Cancelled),
        ("False", "TaskRunTimeout", PipelineStatus::Failed),
        ("False", "TaskRunImagePullFailed", PipelineStatus::Failed),
    ];

    for (status, reason, expected) in cases {
        assert_eq!(
            map_status(&succeeded(status, reason)),
            expected,
            "{status} {reason}"
        );
    }
}

#[test]
fn maps_a_run_without_a_succeeded_condition_to_pending() {
    let conditions: Vec<types::Condition> =
        serde_json::from_str(r#"[{"type":"Ready","status":"True"}]"#).unwrap();

    assert_eq!(map_status(&conditions), PipelineStatus::Pending);
    assert_eq!(map_status(&[]), PipelineStatus::Pending);
}

fn pipeline_run(status_json: &str) -> TektonPipelineRun {
    serde_json::from_str(&format!(
        r#"{{"apiVersion":"tekton.dev/v1","kind":"PipelineRun","metadata":{{"name":"build-1","namespace":"ci"}},"spec":{{}},"status":{status_json}}}"#
    ))
    .unwrap()
}

fn task_run(name: &str, status_json: &str) -> TektonTaskRun {
    serde_json::from_str(&format!(
        r#"{{"metadata":{{"name":"{name}","namespace":"ci"}},"status":{status_json}}}"#
    ))
    .unwrap()
}

const TWO_CHILDREN: &str = r#"{"childReferences":[
    {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-clone","pipelineTaskName":"clone"},
    {"apiVersion":"tekton.dev/v1beta1","kind":"CustomRun","name":"build-1-approval","pipelineTaskName":"approval"},
    {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-test","pipelineTaskName":"test"}
]}"#;

#[test]
fn listed_taskruns_come_from_child_references_then_from_legacy_task_runs() {
    let current = pipeline_run(TWO_CHILDREN);
    assert_eq!(
        listed_taskruns(&current),
        [("build-1-clone", "clone"), ("build-1-test", "test")]
    );

    let legacy = pipeline_run(
        r#"{"taskRuns":{
            "build-1-test":{"pipelineTaskName":"test"},
            "build-1-clone":{"pipelineTaskName":"clone"}
        }}"#,
    );
    assert_eq!(
        listed_taskruns(&legacy),
        [("build-1-clone", "clone"), ("build-1-test", "test")]
    );

    assert!(listed_taskruns(&pipeline_run("{}")).is_empty());
}

#[test]
fn every_taskrun_condition_maps_to_a_job_status_and_log_availability() {
    let run = pipeline_run(TWO_CHILDREN);
    let cases = [
        (
            "True",
            "Succeeded",
            PipelineStatus::Success,
            LogAvailability::Ready,
        ),
        (
            "False",
            "Failed",
            PipelineStatus::Failed,
            LogAvailability::Ready,
        ),
        (
            "False",
            "TaskRunTimeout",
            PipelineStatus::Failed,
            LogAvailability::Ready,
        ),
        (
            "False",
            "TaskRunCancelled",
            PipelineStatus::Cancelled,
            LogAvailability::Ready,
        ),
        (
            "Unknown",
            "Running",
            PipelineStatus::Running,
            LogAvailability::Partial,
        ),
        (
            "Unknown",
            "Started",
            PipelineStatus::Running,
            LogAvailability::Partial,
        ),
        (
            "Unknown",
            "Pending",
            PipelineStatus::Pending,
            LogAvailability::Pending,
        ),
    ];
    for (status, reason, expected, log) in cases {
        let json = format!(
            r#"{{"conditions":[{{"type":"Succeeded","status":"{status}","reason":"{reason}"}}]}}"#
        );
        let nodes = taskruns_to_log_nodes(&run, &[task_run("build-1-clone", &json)]);
        assert_eq!(nodes.len(), 1, "{status} {reason}");
        assert_eq!(nodes[0].status, expected, "{status} {reason}");
        assert_eq!(nodes[0].log, Some(log), "{status} {reason}");
    }

    let nodes = taskruns_to_log_nodes(&run, &[task_run("build-1-clone", "{}")]);
    assert_eq!(nodes[0].status, PipelineStatus::Pending);
    assert_eq!(nodes[0].log, Some(LogAvailability::Pending));
}

#[test]
fn job_nodes_follow_the_child_references_and_skip_unlisted_or_missing_taskruns() {
    let run = pipeline_run(TWO_CHILDREN);
    let task_runs = [
        task_run("build-1-test", r#"{"startTime":"2026-10-28T10:02:00Z"}"#),
        task_run("build-1-stray", "{}"),
        task_run(
            "build-1-clone",
            r#"{"startTime":"2026-10-28T10:00:00Z","completionTime":"2026-10-28T10:01:30Z"}"#,
        ),
    ];

    let nodes = taskruns_to_log_nodes(&run, &task_runs);

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.name.as_str(),
                node.kind,
                node.parent_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("build-1-clone", "clone", LogNodeKind::Job, None),
            ("build-1-test", "test", LogNodeKind::Job, None),
        ]
    );
    assert_eq!(
        nodes[0].started_at,
        types::parse_timestamp(&Some("2026-10-28T10:00:00Z".to_string()))
    );
    assert_eq!(
        nodes[0].finished_at,
        types::parse_timestamp(&Some("2026-10-28T10:01:30Z".to_string()))
    );
    assert_eq!(nodes[1].finished_at, None);
}

#[test]
fn steps_become_child_nodes_without_a_log_of_their_own() {
    let run = pipeline_run(TWO_CHILDREN);
    let steps = r#"{"steps":[
        {"name":"fetch","container":"step-fetch","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:00:01Z","finishedAt":"2026-10-28T10:00:05Z"},"terminationReason":"Completed"},
        {"name":"compile","container":"step-compile","terminated":{"exitCode":2,"startedAt":"2026-10-28T10:00:05Z","finishedAt":"2026-10-28T10:00:09Z"},"terminationReason":"Error"},
        {"name":"package","container":"step-package","terminated":{"exitCode":1,"startedAt":"2026-10-28T10:00:09Z","finishedAt":"2026-10-28T10:00:09Z"},"terminationReason":"Skipped"},
        {"name":"upload","container":"step-upload","running":{"startedAt":"2026-10-28T10:00:10Z"}},
        {"name":"notify","container":"step-notify","waiting":{"reason":"PodInitializing"}}
    ]}"#;

    let nodes = taskruns_to_log_nodes(&run, &[task_run("build-1-clone", steps)]);

    let summary: Vec<_> = nodes[1..]
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.parent_id.as_deref(),
                node.kind,
                node.name.as_str(),
                node.status,
                node.log,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "build-1-clone:fetch",
                Some("build-1-clone"),
                LogNodeKind::Step,
                "fetch",
                PipelineStatus::Success,
                None
            ),
            (
                "build-1-clone:compile",
                Some("build-1-clone"),
                LogNodeKind::Step,
                "compile",
                PipelineStatus::Failed,
                None
            ),
            (
                "build-1-clone:package",
                Some("build-1-clone"),
                LogNodeKind::Step,
                "package",
                PipelineStatus::Skipped,
                None
            ),
            (
                "build-1-clone:upload",
                Some("build-1-clone"),
                LogNodeKind::Step,
                "upload",
                PipelineStatus::Running,
                None
            ),
            (
                "build-1-clone:notify",
                Some("build-1-clone"),
                LogNodeKind::Step,
                "notify",
                PipelineStatus::Pending,
                None
            ),
        ]
    );
    let stamp = |text: &str| types::parse_timestamp(&Some(text.to_string()));
    assert_eq!(nodes[1].started_at, stamp("2026-10-28T10:00:01Z"));
    assert_eq!(nodes[1].finished_at, stamp("2026-10-28T10:00:05Z"));
    assert_eq!(nodes[4].started_at, stamp("2026-10-28T10:00:10Z"));
    assert_eq!(nodes[4].finished_at, None);
    assert_eq!(nodes[5].started_at, None);
}

#[test]
fn sidecars_have_separate_selectable_logs_under_their_task() {
    let run = pipeline_run(TWO_CHILDREN);
    let task = task_run(
        "build-1-test",
        r#"{"conditions":[{"type":"Succeeded","status":"False","reason":"Failed"}],"steps":[{"name":"cache","terminated":{"exitCode":1}}],"sidecars":[
            {"name":"cache","running":{"startedAt":"2026-10-28T10:00:01Z"}},
            {"name":"database","terminated":{"exitCode":2,"startedAt":"2026-10-28T10:00:02Z","finishedAt":"2026-10-28T10:00:03Z"}},
            {"name":"proxy","terminated":{"exitCode":0}},
            {"name":"pending","waiting":{"reason":"ContainerCreating"}}
        ]}"#,
    );
    let nodes = taskruns_to_log_nodes(&run, &[task]);
    let summary: Vec<_> = nodes[2..]
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.name.as_str(),
                node.parent_id.as_deref(),
                node.kind,
                node.status,
                node.log,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "build-1-test:sidecar:cache",
                "Sidecar: cache",
                Some("build-1-test"),
                LogNodeKind::Step,
                PipelineStatus::Running,
                Some(LogAvailability::Partial)
            ),
            (
                "build-1-test:sidecar:database",
                "Sidecar: database",
                Some("build-1-test"),
                LogNodeKind::Step,
                PipelineStatus::Failed,
                Some(LogAvailability::Ready)
            ),
            (
                "build-1-test:sidecar:proxy",
                "Sidecar: proxy",
                Some("build-1-test"),
                LogNodeKind::Step,
                PipelineStatus::Success,
                Some(LogAvailability::Ready)
            ),
            (
                "build-1-test:sidecar:pending",
                "Sidecar: pending",
                Some("build-1-test"),
                LogNodeKind::Step,
                PipelineStatus::Pending,
                Some(LogAvailability::Pending)
            ),
        ]
    );
    assert_ne!(nodes[1].id, nodes[2].id);
    assert_eq!(nodes[1].log, None);
    assert_eq!(
        nodes[3].started_at.unwrap().to_rfc3339(),
        "2026-10-28T10:00:02+00:00"
    );
    assert_eq!(
        nodes[3].finished_at.unwrap().to_rfc3339(),
        "2026-10-28T10:00:03+00:00"
    );
}
