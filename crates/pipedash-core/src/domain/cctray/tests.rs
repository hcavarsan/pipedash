use std::collections::HashMap;

use chrono::{
    TimeZone,
    Utc,
};

use super::*;

fn pipeline(id: &str, name: &str, repository: &str, status: PipelineStatus) -> Pipeline {
    Pipeline {
        id: id.into(),
        provider_id: 1,
        provider_type: "svc".into(),
        name: name.into(),
        status,
        last_run: Some(Utc.with_ymd_and_hms(2026, 11, 16, 12, 30, 0).unwrap()),
        last_updated: Utc.with_ymd_and_hms(2026, 11, 16, 13, 0, 0).unwrap(),
        repository: repository.into(),
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata: HashMap::new(),
        overdue_run: None,
        stuck_queue_run: None,
        typical_duration_seconds: None,
        cadence: None,
        actions: Default::default(),
    }
}

#[test]
fn feed_lists_each_pipeline_with_status_activity_time_and_link() {
    let pipelines = [
        pipeline("svc__1__ci.yml", "ci", "acme/api", PipelineStatus::Success),
        pipeline("svc__1__deploy", "deploy", "", PipelineStatus::Running),
    ];

    let xml = cctray_feed(&pipelines, "https://dash.example.com");

    assert_eq!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Projects>\n  \
         <Project name=\"acme/api · ci\" activity=\"Sleeping\" lastBuildStatus=\"Success\" \
         lastBuildTime=\"2026-11-16T12:30:00+00:00\" \
         webUrl=\"https://dash.example.com/pipelines/svc__1__ci.yml\"/>\n  \
         <Project name=\"deploy\" activity=\"Building\" lastBuildStatus=\"Unknown\" \
         lastBuildTime=\"2026-11-16T12:30:00+00:00\" \
         webUrl=\"https://dash.example.com/pipelines/svc__1__deploy\"/>\n</Projects>\n"
    );
}

#[test]
fn failed_and_degraded_pipelines_are_failures() {
    for status in [PipelineStatus::Failed, PipelineStatus::Degraded] {
        let xml = cctray_feed(&[pipeline("id", "n", "", status)], "http://h");
        assert!(
            xml.contains("activity=\"Sleeping\" lastBuildStatus=\"Failure\""),
            "{xml}"
        );
    }
}

#[test]
fn names_and_links_are_escaped() {
    let xml = cctray_feed(
        &[pipeline(
            "svc__1__a/b?c",
            "<\"x\" & 'y'>",
            "",
            PipelineStatus::Success,
        )],
        "http://h:8080",
    );

    assert!(
        xml.contains("name=\"&lt;&quot;x&quot; &amp; &apos;y&apos;&gt;\""),
        "{xml}"
    );
    assert!(
        xml.contains("webUrl=\"http://h:8080/pipelines/svc__1__a%2Fb%3Fc\""),
        "{xml}"
    );
}

#[test]
fn a_pipeline_that_never_ran_sleeps_and_uses_its_last_update() {
    let mut never_ran = pipeline("id", "n", "", PipelineStatus::Pending);
    never_ran.last_run = None;

    let xml = cctray_feed(&[never_ran], "not a url");

    assert!(
        xml.contains(
            "activity=\"Sleeping\" lastBuildStatus=\"Unknown\" \
             lastBuildTime=\"2026-11-16T13:00:00+00:00\"/>"
        ),
        "{xml}"
    );
}

#[test]
fn links_keep_the_frontend_base_path() {
    let xml = cctray_feed(
        &[pipeline("svc__1__ci", "ci", "", PipelineStatus::Success)],
        "https://h/pipedash/",
    );

    assert!(
        xml.contains("webUrl=\"https://h/pipedash/pipelines/svc__1__ci\""),
        "{xml}"
    );
}

#[test]
fn pipelines_sharing_a_name_get_their_workflow_file_or_id() {
    let mut first = pipeline("svc__1__a.yml", "ci", "acme/api", PipelineStatus::Success);
    first.workflow_file = Some("pipelines/a.yml".into());
    let second = pipeline("svc__1__b.yml", "ci", "acme/api", PipelineStatus::Success);
    let other = pipeline("svc__1__c.yml", "lint", "acme/api", PipelineStatus::Success);

    let xml = cctray_feed(&[first, second, other], "http://h");

    assert!(
        xml.contains("name=\"acme/api · ci · pipelines/a.yml\""),
        "{xml}"
    );
    assert!(
        xml.contains("name=\"acme/api · ci · svc__1__b.yml\""),
        "{xml}"
    );
    assert!(xml.contains("name=\"acme/api · lint\""), "{xml}");
}
