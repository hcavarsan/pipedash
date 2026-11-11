use super::*;

fn deployments() -> Vec<types::Deployment> {
    serde_json::from_value(serde_json::json!([
        {"environment": "production", "created_at": "2026-10-20T10:05:00Z"},
        {"environment": "production", "created_at": "2026-10-20T10:06:00Z"},
        {
            "environment": "preview-7", "created_at": "2026-10-20T10:04:00Z",
            "transient_environment": true
        },
        {"environment": "staging", "created_at": "2026-10-19T10:04:00Z"},
        {"environment": "", "created_at": "2026-10-20T10:04:00Z"}
    ]))
    .unwrap()
}

fn at(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

#[test]
fn deployments_inside_the_run_window_become_sorted_targets() {
    let cluster = ClusterId::from_host("github.test/acme/app").unwrap();

    let targets = deployment_targets(
        &cluster,
        at("2026-10-20T10:00:00Z"),
        at("2026-10-20T10:10:00Z"),
        &deployments(),
    );

    assert_eq!(
        targets,
        [
            TargetObservation {
                cluster: cluster.clone(),
                namespace: None,
                name: "preview-7".to_string(),
                lifetime: TargetLifetime::ShortLived,
            },
            TargetObservation {
                cluster,
                namespace: None,
                name: "production".to_string(),
                lifetime: TargetLifetime::Standing,
            },
        ]
    );
}

#[test]
fn deployments_outside_the_run_window_are_not_the_runs() {
    let cluster = ClusterId::from_host("github.test/acme/app").unwrap();

    let targets = deployment_targets(
        &cluster,
        at("2026-10-20T11:00:00Z"),
        at("2026-10-20T11:10:00Z"),
        &deployments(),
    );

    assert!(targets.is_empty());
}

#[test]
fn a_pipeline_without_a_repository_has_no_sources() {
    assert_eq!(
        pipeline_correlation(None, Vec::new()),
        Correlation::default()
    );
    let repo = RepoId::from_host_path("github.test", "acme/app");

    let correlation = pipeline_correlation(repo.clone(), vec![PathScope::new("src")]);

    assert_eq!(
        correlation.sources,
        [SourceObservation {
            repo: repo.unwrap(),
            path_scopes: vec![PathScope::new("src")],
            commit: None,
        }]
    );
}

fn commit(files: serde_json::Value) -> types::CommitResponse {
    serde_json::from_value(serde_json::json!({
        "sha": "0123456789abcdef0123456789abcdef01234567",
        "commit": {
            "message": "ship it",
            "author": {"name": "Ada", "date": "2026-10-20T09:00:00Z"},
            "committer": {"name": "Bob", "date": "2026-10-20T09:30:00Z"}
        },
        "parents": [{"sha": "89abcdef0123456789abcdef0123456789abcdef"}, {"sha": "short"}],
        "files": files
    }))
    .unwrap()
}

#[test]
fn commit_info_keeps_both_dates_and_valid_parents() {
    let info = commit_info(commit(serde_json::json!([]))).unwrap();

    assert_eq!(
        info,
        CommitInfo {
            sha: CommitSha::parse("0123456789abcdef0123456789abcdef01234567").unwrap(),
            authored_at: at("2026-10-20T09:00:00Z"),
            committed_at: at("2026-10-20T09:30:00Z"),
            message: "ship it".to_string(),
            parents: vec![CommitSha::parse("89abcdef0123456789abcdef0123456789abcdef").unwrap()],
        }
    );
}

#[test]
fn commit_info_without_a_date_is_an_api_error() {
    let commit: types::CommitResponse = serde_json::from_value(serde_json::json!({
        "sha": "0123456789abcdef0123456789abcdef01234567",
        "commit": {"message": "m", "author": {"date": "2026-10-20T09:00:00Z"}, "committer": null}
    }))
    .unwrap();

    assert!(matches!(commit_info(commit), Err(PluginError::ApiError(_))));
}

#[test]
fn changed_files_keep_paths_and_patches() {
    let files = changed_files(commit(serde_json::json!([
        {"filename": "deploy/values.yaml", "patch": "+image: ghcr.io/acme/app:1"},
        {"filename": "logo.png"}
    ])));

    assert_eq!(
        files,
        [
            ChangedFile {
                path: "deploy/values.yaml".to_string(),
                patch: Some("+image: ghcr.io/acme/app:1".to_string()),
            },
            ChangedFile {
                path: "logo.png".to_string(),
                patch: None,
            },
        ]
    );
}

#[test]
fn comparison_maps_counts_and_valid_commits() {
    let comparison: types::Comparison = serde_json::from_value(serde_json::json!({
        "ahead_by": 2, "behind_by": 1,
        "commits": [
            {"sha": "0123456789abcdef0123456789abcdef01234567"},
            {"sha": "nope"}
        ]
    }))
    .unwrap();

    assert_eq!(
        commit_comparison(comparison),
        CommitComparison {
            ahead: 2,
            behind: 1,
            commits: vec![CommitSha::parse("0123456789abcdef0123456789abcdef01234567").unwrap()],
        }
    );
}
