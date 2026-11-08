use pipedash_core::domain::notification_center::CenterTone;
use pipedash_plugin_api::build_watch::BuildWatchMonitoring;

use super::*;

fn host_alert(kind: &str, created_at: &str) -> HostAlert {
    HostAlert {
        id: "a1".to_string(),
        seq: 1,
        key: "pipeline:1".to_string(),
        kind: kind.to_string(),
        tone: "danger".to_string(),
        title: "Failed".to_string(),
        body: "api".to_string(),
        pipeline_id: Some("stub__1__api".to_string()),
        provider_id: Some(7),
        provider_ids: Vec::new(),
        finish: None,
        created_at: created_at.to_string(),
    }
}

fn watch(status: BuildWatchRunStatus) -> BuildWatchSnapshot {
    BuildWatchSnapshot {
        session_id: "s1".to_string(),
        pipeline_id: "stub__1__api".to_string(),
        pipeline_name: "api".to_string(),
        provider_name: "Stub".to_string(),
        provider_type: "stub".to_string(),
        run_number: 42,
        branch: None,
        status,
        started_at: 1,
        concluded_at: None,
        updated_at: 2,
        monitoring: BuildWatchMonitoring::Completed,
        url: String::new(),
    }
}

#[test]
fn a_missed_alert_maps_its_kind_time_and_affected_providers() {
    let alert = host_alert("outage", "2023-11-14T22:13:20Z");
    let notice = missed(&alert).unwrap();

    assert_eq!(notice.key, "pipeline:1");
    assert_eq!(notice.at, 1_700_000_000_000);
    assert_eq!(notice.tone, CenterTone::Warning);
    assert!(!notice.read);
    assert_eq!(notice.provider_ids, vec![7]);
    assert_eq!(notice.pipeline_id.as_deref(), Some("stub__1__api"));

    let several = HostAlert {
        provider_ids: vec![1, 2],
        ..alert
    };
    assert_eq!(missed(&several).unwrap().provider_ids, vec![1, 2]);
}

#[test]
fn a_missed_alert_with_an_unknown_kind_or_time_is_ignored() {
    assert!(missed(&host_alert("nope", "2023-11-14T22:13:20Z")).is_none());
    assert!(missed(&host_alert("failed", "yesterday")).is_none());
}

#[test]
fn a_watch_snapshot_maps_its_status_to_a_notice() {
    for (status, tone, title) in [
        (
            BuildWatchRunStatus::Success,
            CenterTone::Success,
            "Watched run #42 succeeded",
        ),
        (
            BuildWatchRunStatus::Failed,
            CenterTone::Danger,
            "Watched run #42 failed",
        ),
        (
            BuildWatchRunStatus::Cancelled,
            CenterTone::Warning,
            "Watched run #42 was cancelled",
        ),
        (
            BuildWatchRunStatus::Skipped,
            CenterTone::Warning,
            "Watched run #42 was skipped",
        ),
        (
            BuildWatchRunStatus::Running,
            CenterTone::Warning,
            "Watched run #42 finished",
        ),
    ] {
        let notice = watched(&watch(status));

        assert_eq!(notice.key, "watch:s1");
        assert_eq!(notice.tone, tone);
        assert_eq!(notice.title, title);
        assert_eq!(notice.body, "api · Stub");
        assert_eq!(notice.run_number, Some(42));
    }
}

#[test]
fn a_notice_targets_its_first_provider_and_run() {
    let mut notice = watched(&watch(BuildWatchRunStatus::Success));
    notice.provider_ids = vec![5, 6];

    let target = target(&notice);

    assert_eq!(target.key.as_deref(), Some("watch:s1"));
    assert_eq!(target.pipeline_id.as_deref(), Some("stub__1__api"));
    assert_eq!(target.provider_id, Some(5));
    assert_eq!(target.run_number, Some(42));
}
