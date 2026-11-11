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
    let mapped = map_run(&run, "5", String::new(), None);
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

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn repo(value: serde_json::Value) -> types::Repo {
    serde_json::from_value(value).unwrap()
}

fn forge() -> RepoId {
    RepoId::from_host_path("forge.test", "acme/web").unwrap()
}

fn run_at(commit: &str) -> types::Pipeline {
    pipeline(json!({"id": 9, "number": 4, "status": "success", "commit": commit}))
}

#[test]
fn run_observes_the_repository_and_its_commit() {
    let source = forge();

    let mapped = map_run(&run_at(SHA), "5", String::new(), Some(&source));

    assert_eq!(
        mapped.correlation,
        Correlation::single_source(Some(forge()), CommitSha::parse(SHA))
    );
    assert_eq!(mapped.evidence, RunEvidence::default());
    assert_eq!(
        map_run(&run_at(SHA), "5", String::new(), Some(&source)).correlation,
        mapped.correlation
    );
}

#[test]
fn run_without_a_repository_or_a_commit_id_omits_those_observations() {
    let source = forge();
    assert!(map_run(&run_at(SHA), "5", String::new(), None)
        .correlation
        .is_empty());
    assert_eq!(
        map_run(&run_at("abc"), "5", String::new(), Some(&source)).correlation,
        Correlation::single_source(Some(forge()), None)
    );
}

#[test]
fn repository_comes_from_the_forge_url_then_the_clone_url() {
    let base = |extra: serde_json::Value| {
        let mut value = json!({"id": 5, "owner": "acme", "name": "web", "full_name": "acme/web"});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        repo(value)
    };

    for extra in [
        json!({"forge_url": "https://forge.test/acme/web", "clone_url": "https://other.test/x/y.git"}),
        json!({"forge_url": "", "clone_url": "https://forge.test/acme/web.git"}),
        json!({"clone_url": "git@forge.test:acme/web.git"}),
    ] {
        assert_eq!(source_repo(&base(extra.clone())), Some(forge()), "{extra}");
    }
    assert_eq!(source_repo(&base(json!({}))), None);
    assert_eq!(
        map_pipeline(
            &base(json!({"forge_url": "https://forge.test/acme/web"})),
            None
        )
        .correlation,
        Correlation::single_source(Some(forge()), None)
    );
}
