use super::*;
#[test]
fn status_mapping_follows_github_style_status_and_conclusion() {
    let cases = [
        ("completed", Some("success"), PipelineStatus::Success),
        ("completed", Some("failure"), PipelineStatus::Failed),
        ("completed", Some("cancelled"), PipelineStatus::Cancelled),
        ("completed", Some("skipped"), PipelineStatus::Skipped),
        ("in_progress", None, PipelineStatus::Running),
        ("queued", None, PipelineStatus::Running),
        ("waiting", None, PipelineStatus::Pending),
        ("unknown", None, PipelineStatus::Pending),
    ];
    for (status, conclusion, expected) in cases {
        assert_eq!(map_status(status, conclusion), expected, "{status}");
    }
}

#[test]
fn epoch_zero_timestamps_are_treated_as_unset() {
    let run: types::Run = serde_json::from_value(serde_json::json!({
        "id": 2, "run_number": 2, "status": "queued", "path": "ci.yaml@refs/heads/main",
        "html_url": "http://g/o/r/actions/runs/2",
        "started_at": "1970-01-01T00:00:00Z", "completed_at": "1970-01-01T00:00:00Z"
    }))
    .unwrap();
    let mapped = map_run(&run, "o/r/ci.yaml", "o", None);
    assert_eq!(mapped.concluded_at, None);
    assert_eq!(mapped.duration_seconds, None);
    assert_eq!(run.workflow_file(), "ci.yaml");
}

fn jobs(value: serde_json::Value) -> Vec<types::Job> {
    serde_json::from_value::<types::JobList>(value)
        .unwrap()
        .jobs
}

#[test]
fn every_job_status_word_maps_to_a_status_and_a_log_availability() {
    let started = "2026-10-28T10:00:00Z";
    let unset = "1970-01-01T00:00:00Z";
    let cases = [
        (
            "completed",
            "success",
            started,
            PipelineStatus::Success,
            Some(LogAvailability::Ready),
        ),
        (
            "completed",
            "failure",
            started,
            PipelineStatus::Failed,
            Some(LogAvailability::Ready),
        ),
        (
            "completed",
            "cancelled",
            started,
            PipelineStatus::Cancelled,
            Some(LogAvailability::Ready),
        ),
        (
            "completed",
            "cancelled",
            unset,
            PipelineStatus::Cancelled,
            None,
        ),
        ("completed", "skipped", unset, PipelineStatus::Skipped, None),
        (
            "in_progress",
            "",
            started,
            PipelineStatus::Running,
            Some(LogAvailability::Partial),
        ),
        (
            "queued",
            "",
            unset,
            PipelineStatus::Running,
            Some(LogAvailability::Pending),
        ),
        (
            "waiting",
            "",
            unset,
            PipelineStatus::Pending,
            Some(LogAvailability::Pending),
        ),
        (
            "",
            "",
            unset,
            PipelineStatus::Pending,
            Some(LogAvailability::Pending),
        ),
    ];
    let list = cases
        .iter()
        .enumerate()
        .map(|(i, (status, conclusion, started_at, ..))| {
            serde_json::json!({
                "id": i, "name": format!("job{i}"), "status": status,
                "conclusion": Some(conclusion).filter(|c| !c.is_empty()),
                "started_at": started_at, "completed_at": unset,
            })
        })
        .collect::<Vec<_>>();

    let nodes = jobs_to_log_nodes(&jobs(serde_json::json!({ "jobs": list })));

    assert_eq!(nodes.len(), cases.len());
    for (node, (status, conclusion, _, expected_status, expected_log)) in nodes.iter().zip(cases) {
        let word = format!("{status}/{conclusion}");
        assert_eq!(node.status, expected_status, "{word}");
        assert_eq!(node.log, expected_log, "{word}");
    }
}

#[test]
fn steps_become_children_of_their_job_without_a_log_of_their_own() {
    let nodes = jobs_to_log_nodes(&jobs(serde_json::json!({"jobs": [
        {
            "id": 101, "name": "build", "status": "completed", "conclusion": "failure",
            "html_url": "http://g/o/r/actions/runs/7/jobs/0",
            "started_at": "2026-10-28T10:00:00Z", "completed_at": "2026-10-28T10:01:00Z",
            "steps": [
                {"name": "Set up job", "number": 0, "status": "completed",
                 "conclusion": "success", "started_at": "2026-10-28T10:00:01Z",
                 "completed_at": "2026-10-28T10:00:03Z"},
                {"name": "Run tests", "number": 1, "status": "completed",
                 "conclusion": "failure", "started_at": "2026-10-28T10:00:03Z",
                 "completed_at": "2026-10-28T10:00:58Z"}
            ]
        },
        {
            "id": 102, "name": "deploy", "status": "queued", "steps": null,
            "started_at": "1970-01-01T00:00:00Z", "completed_at": "1970-01-01T00:00:00Z"
        }
    ]})));

    let shape: Vec<_> = nodes
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
        shape,
        [
            ("101", None, LogNodeKind::Job, Some(LogAvailability::Ready)),
            ("101:0", Some("101"), LogNodeKind::Step, None),
            ("101:1", Some("101"), LogNodeKind::Step, None),
            (
                "102",
                None,
                LogNodeKind::Job,
                Some(LogAvailability::Pending)
            ),
        ]
    );
    assert_eq!(
        nodes[0].web_url.as_deref(),
        Some("http://g/o/r/actions/runs/7/jobs/0")
    );
    assert_eq!(nodes[1].name, "Set up job");
    assert_eq!(nodes[1].status, PipelineStatus::Success);
    assert_eq!(nodes[2].status, PipelineStatus::Failed);
    assert_eq!(
        nodes[2].started_at,
        Some("2026-10-28T10:00:03Z".parse().unwrap())
    );
    assert_eq!(
        nodes[2].finished_at,
        Some("2026-10-28T10:00:58Z".parse().unwrap())
    );
    assert_eq!(nodes[3].started_at, None);
    assert_eq!(nodes[3].finished_at, None);
    assert_eq!(nodes[3].web_url, None);
}

#[test]
fn pipeline_and_runs_carry_repository_commit_and_timing_evidence() {
    let (pipeline, runs) = crate::fixtures::correlation_fixtures().remove(0);
    let repo = RepoId::from_host_path(crate::fixtures::HOST, "acme/api");
    let commit = CommitSha::parse(crate::fixtures::COMMIT);

    assert_eq!(
        pipeline.correlation,
        Correlation::single_source(repo.clone(), None)
    );
    assert_eq!(
        runs[0].correlation,
        Correlation::single_source(repo.clone(), commit.clone())
    );
    assert_eq!(runs[0].evidence, RunEvidence::default());
    assert_eq!(
        runs[1].correlation,
        Correlation::single_source(repo, commit)
    );
    assert_eq!(
        runs[1].evidence,
        RunEvidence {
            timing: Observed::Absent,
            ..RunEvidence::default()
        }
    );
    assert_eq!(runs[1].duration_seconds, None);
}

#[test]
fn an_unparseable_head_sha_keeps_the_repository_without_a_commit() {
    let source = RepoId::from_host_path(crate::fixtures::HOST, "acme/api");
    let raw: types::Run = serde_json::from_value(serde_json::json!({
        "id": 3, "run_number": 3, "status": "queued", "path": "ci.yaml",
        "html_url": "http://g/o/r/actions/runs/3", "head_sha": "abc"
    }))
    .unwrap();

    let mapped = map_run(&raw, "o/r/ci.yaml", "o", source.clone());

    assert_eq!(mapped.correlation, Correlation::single_source(source, None));
}

#[test]
fn commit_payloads_become_commit_info_comparisons_and_changed_files() {
    let commit: types::Commit = serde_json::from_value(serde_json::json!({
        "sha": crate::fixtures::COMMIT,
        "commit": {
            "message": "fix it",
            "author": {"date": "2026-10-01T12:00:00+02:00"},
            "committer": {"date": "2026-10-01T11:00:00Z"}
        },
        "parents": [{"sha": "fedcba9876543210fedcba9876543210fedcba98"}, {"sha": "bad"}],
        "files": [{"filename": "deploy/values.yaml", "status": "modified"}]
    }))
    .unwrap();

    let info = commits::commit_info(&commit).unwrap();
    assert_eq!(info.sha.as_str(), crate::fixtures::COMMIT);
    assert_eq!(info.authored_at.to_rfc3339(), "2026-10-01T10:00:00+00:00");
    assert_eq!(info.committed_at.to_rfc3339(), "2026-10-01T11:00:00+00:00");
    assert_eq!(info.parents.len(), 1);

    let changes = commits::changed_files(commit);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "deploy/values.yaml");
    assert_eq!(changes[0].patch, None);

    let compare: types::Compare = serde_json::from_value(serde_json::json!({
        "total_commits": 3, "commits": [{"sha": crate::fixtures::COMMIT}, {"sha": "bad"}]
    }))
    .unwrap();
    let behind = types::Compare {
        total_commits: 1,
        commits: Vec::new(),
    };
    let comparison = commits::comparison(&compare, &behind);
    assert_eq!((comparison.ahead, comparison.behind), (3, 1));
    assert_eq!(comparison.commits.len(), 1);
}
