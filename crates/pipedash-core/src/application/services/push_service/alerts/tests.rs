use chrono::Utc;

use super::*;
use crate::domain::ResourceState;

fn pipeline(name: &str, branch: Option<&str>) -> Pipeline {
    serde_json::from_value(serde_json::json!({
        "id": "stub__1__acme/app/ci.yml",
        "provider_id": 1,
        "provider_type": "stub",
        "name": name,
        "status": "failed",
        "last_run": null,
        "last_updated": Utc::now(),
        "repository": "acme/app",
        "branch": branch,
        "workflow_file": null,
    }))
    .unwrap()
}

fn run(number: i64, branch: Option<&str>) -> PipelineRun {
    serde_json::from_value(serde_json::json!({
        "id": number.to_string(),
        "pipeline_id": "stub__1__acme/app/ci.yml",
        "run_number": number,
        "status": "failed",
        "started_at": Utc::now(),
        "logs_url": "",
        "branch": branch,
    }))
    .unwrap()
}

#[test]
fn a_failure_names_the_pipeline_repository_provider_branch_run_and_thread() {
    let alert = failure_alert(&pipeline("CI", Some("main")), "Stub", Some(&run(42, None)));

    assert_eq!(alert.title, "CI failed · acme/app");
    assert_eq!(alert.body, "Stub · main · #42");
    assert_eq!(alert.thread_id.as_deref(), Some("stub__1__acme/app/ci.yml"));
    assert_eq!(
        alert.pipeline_id.as_deref(),
        Some("stub__1__acme/app/ci.yml")
    );
    assert_eq!(alert.provider_id, Some(1));
    assert_eq!(alert.run_number, Some(42));
}

#[test]
fn the_run_branch_wins_and_missing_parts_are_left_out() {
    let with_run_branch = failure_alert(
        &pipeline("CI", Some("main")),
        "Stub",
        Some(&run(7, Some("release"))),
    );
    let provider_only = failure_alert(&pipeline("CI", None), "Stub", None);

    assert_eq!(with_run_branch.body, "Stub · release · #7");
    assert_eq!(provider_only.body, "Stub");
    assert_eq!(provider_only.run_number, None);
}

#[test]
fn long_names_and_repositories_are_cut_so_the_relay_accepts_them() {
    let alert = failure_alert(
        &pipeline(&"é".repeat(500), None),
        "p".repeat(1000).as_str(),
        None,
    );
    let mut far = pipeline("CI", None);
    far.repository = "r".repeat(500);
    let distant = failure_alert(&far, "Stub", None);

    assert_eq!(alert.title.chars().count(), MAX_TITLE_CHARS);
    assert!(alert.title.ends_with("… failed · acme/app"));
    assert!(distant.title.starts_with("CI failed · rrr"));
    assert!(distant.title.ends_with('…'));
    assert!(distant.title.chars().count() <= MAX_TITLE_CHARS);
    assert_eq!(alert.body.chars().count(), MAX_BODY_CHARS);
    assert_eq!(truncate("short", 10), "short");
}

#[test]
fn a_degraded_pipeline_says_so_and_explains_why() {
    let mut degraded = pipeline("web", None);
    degraded.status = PipelineStatus::Degraded;
    degraded.resource_state = Some(ResourceState::new(
        "Degraded",
        Some("Pod web/web-7f9c: back-off restarting failed container"),
    ));

    let alert = failure_alert(&degraded, "Stub", None);

    assert_eq!(alert.title, "web is degraded · acme/app");
    assert_eq!(
        alert.body,
        "Stub · Pod web/web-7f9c: back-off restarting failed container"
    );
    assert_eq!(alert.run_number, None);
}
