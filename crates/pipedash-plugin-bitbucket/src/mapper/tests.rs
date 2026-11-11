use pipedash_plugin_api::PluginError;

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

#[test]
fn pipeline_and_runs_carry_repository_and_commit() {
    let (pipeline, runs) = crate::fixtures::correlation_fixtures().remove(0);
    let repo = RepoId::parse("https://bitbucket.org/acme/api");

    assert_eq!(
        pipeline.correlation,
        Correlation::single_source(repo.clone(), None)
    );
    assert_eq!(
        runs[0].correlation,
        Correlation::single_source(repo.clone(), CommitSha::parse(crate::fixtures::COMMIT))
    );
    assert_eq!(runs[1].correlation, Correlation::single_source(repo, None));
    assert!(runs
        .iter()
        .all(|run| run.evidence == RunEvidence::default()));
}

#[test]
fn diff_chunks_are_matched_to_the_files_the_diffstat_names() {
    let stat = |old: Option<&str>, new: Option<&str>| types::DiffStat {
        old: old.map(|path| types::DiffPath {
            path: path.to_string(),
        }),
        new: new.map(|path| types::DiffPath {
            path: path.to_string(),
        }),
    };
    let stats = [
        stat(Some("a b.txt"), Some("c.txt")),
        stat(Some("gone.txt"), None),
        stat(None, Some("img.bin")),
        stat(None, Some("absent-from-diff.txt")),
    ];
    let diff = "diff --git a/a b.txt b/c.txt\nsimilarity index 90%\nrename from a b.txt\nrename to c.txt\n--- a/a b.txt\n+++ b/c.txt\n@@ -1 +1 @@\n-x\n+++ b/fake\n\
diff --git a/gone.txt b/gone.txt\ndeleted file mode 100644\n--- a/gone.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-y\n\
diff --git a/img.bin b/img.bin\nBinary files differ\n";

    let files = commits::changed_files(&stats, diff);

    let patches: Vec<_> = files
        .iter()
        .map(|file| (file.path.as_str(), file.patch.as_deref()))
        .collect();
    assert_eq!(
        patches,
        [
            ("c.txt", Some("@@ -1 +1 @@\n-x\n+++ b/fake\n")),
            ("gone.txt", Some("@@ -1 +0,0 @@\n-y\n")),
            ("img.bin", None),
            ("absent-from-diff.txt", None),
        ]
    );
}

#[test]
fn commit_payloads_become_commit_info_and_comparisons() {
    let commit: types::Commit = serde_json::from_value(serde_json::json!({
        "hash": crate::fixtures::COMMIT, "date": "2026-10-01T12:00:00+02:00",
        "message": "fix it", "parents": [{"hash": "bad"}]
    }))
    .unwrap();

    let info = commits::commit_info(&commit).unwrap();
    assert_eq!(info.committed_at.to_rfc3339(), "2026-10-01T10:00:00+00:00");
    assert!(info.parents.is_empty());

    let hash = |value: &str| types::CommitHash {
        hash: value.to_string(),
    };
    let comparison =
        commits::comparison(&[hash(crate::fixtures::COMMIT), hash("bad")], &[hash("x")]);
    assert_eq!((comparison.ahead, comparison.behind), (2, 1));
    assert_eq!(comparison.commits.len(), 1);

    let mut invalid = commit;
    invalid.hash = "bad".to_string();
    assert!(matches!(
        commits::commit_info(&invalid),
        Err(PluginError::ApiError(_))
    ));
}
