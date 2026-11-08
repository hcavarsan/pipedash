use super::*;

fn snapshot() -> BuildWatchSnapshot {
    BuildWatchSnapshot {
        session_id: "session-1".to_string(),
        pipeline_id: "stub__1__owner/repo/build.yml".to_string(),
        pipeline_name: "Build".to_string(),
        provider_name: "Stub".to_string(),
        provider_type: "stub".to_string(),
        run_number: 7,
        branch: None,
        status: BuildWatchRunStatus::Running,
        started_at: 100,
        concluded_at: None,
        updated_at: 110,
        monitoring: BuildWatchMonitoring::Watching,
        url: BuildWatchSnapshot::run_url("stub__1__owner/repo/build.yml", 7),
    }
}

#[test]
fn build_watch_serialization_preserves_nulls_and_camel_case() {
    let value = serde_json::to_value(snapshot()).unwrap();
    assert_eq!(value["sessionId"], "session-1");
    assert_eq!(value["runNumber"], 7);
    assert_eq!(value["monitoring"], "watching");
    assert_eq!(value["branch"], serde_json::Value::Null);
    assert_eq!(value["concludedAt"], serde_json::Value::Null);
    assert!(value.get("session_id").is_none());
}

#[test]
fn run_links_encode_pipeline_ids_as_one_path_segment() {
    assert_eq!(
        BuildWatchSnapshot::run_url("stub__1__é/a%?#", 7),
        "pipedash://pipelines/stub__1__%C3%A9%2Fa%25%3F%23/runs/7"
    );
}

#[test]
fn build_watch_rejects_invalid_identity_and_untrusted_urls() {
    let mut watch = snapshot();
    assert!(watch.validate().is_ok());
    watch.run_number = 9_007_199_254_740_992;
    assert!(watch.validate().is_err());
    watch = snapshot();
    watch.url = "https://example.com/token".to_string();
    assert!(watch.validate().is_err());
    watch = snapshot();
    watch.pipeline_id.clear();
    assert!(watch.validate().is_err());
}

#[test]
fn build_watch_rejects_oversized_activity_content() {
    let mut watch = snapshot();
    watch.pipeline_id = "a".repeat(1600);
    watch.url = BuildWatchSnapshot::run_url(&watch.pipeline_id, watch.run_number);
    watch.branch = Some("b".repeat(1024));
    assert_eq!(
        watch.validate().unwrap_err(),
        "The build details exceed the watch size limit"
    );
}

#[test]
fn completed_build_watch_requires_a_terminal_provider_status() {
    let mut watch = snapshot();
    watch.monitoring = BuildWatchMonitoring::Completed;
    assert!(watch.validate().is_err());
    watch.status = BuildWatchRunStatus::Success;
    watch.concluded_at = Some(120);
    assert!(watch.validate().is_ok());
    watch.monitoring = BuildWatchMonitoring::Watching;
    assert!(watch.validate().is_err());
    watch.monitoring = BuildWatchMonitoring::Interrupted;
    assert!(watch.validate().is_ok());
}
