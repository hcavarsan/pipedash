use super::*;

fn source_for(url: &str) -> PluginResult<Option<StatusSource>> {
    let mut config = HashMap::new();
    config.insert(STATUS_PAGE_URL_KEY.to_string(), url.to_string());
    custom_status_source(&config)
}

fn controller(name: &str) -> ComponentHealth {
    ComponentHealth {
        name: name.to_string(),
        level: HealthLevel::MajorOutage,
        detail: Some("scaled to zero".to_string()),
    }
}

#[test]
fn controller_health_leaves_out_healthy_and_single_missing_controllers() {
    let result = collect_controller_health(vec![
        Ok(None),
        Ok(Some(controller("kustomize-controller"))),
        Err(PluginError::PipelineNotFound("helm-controller".into())),
    ]);

    assert_eq!(result.unwrap(), vec![controller("kustomize-controller")]);
}

#[test]
fn any_denied_controller_makes_the_check_denied() {
    let result = collect_controller_health(vec![
        Ok(Some(controller("kustomize-controller"))),
        Err(PluginError::PermissionDenied("forbidden".into())),
    ]);

    assert!(matches!(result, Err(PluginError::PermissionDenied(_))));
}

#[test]
fn every_controller_missing_means_not_installed() {
    let result = collect_controller_health(vec![
        Err(PluginError::PipelineNotFound("a".into())),
        Err(PluginError::PipelineNotFound("b".into())),
    ]);

    assert!(matches!(result, Err(PluginError::PipelineNotFound(_))));
}

#[test]
fn other_errors_fail_the_whole_check() {
    let result = collect_controller_health(vec![
        Err(PluginError::PermissionDenied("forbidden".into())),
        Err(PluginError::ServiceUnavailable {
            message: "503".into(),
            retry_after: None,
        }),
    ]);

    assert!(matches!(
        result,
        Err(PluginError::ServiceUnavailable { .. })
    ));
}

#[test]
fn health_levels_order_by_severity() {
    assert!(HealthLevel::Operational < HealthLevel::Maintenance);
    assert!(HealthLevel::Maintenance < HealthLevel::Degraded);
    assert!(HealthLevel::Degraded < HealthLevel::PartialOutage);
    assert!(HealthLevel::PartialOutage < HealthLevel::MajorOutage);
}

#[test]
fn custom_source_is_absent_when_unset_or_blank() {
    assert_eq!(custom_status_source(&HashMap::new()).unwrap(), None);
    assert_eq!(source_for("   ").unwrap(), None);
}

#[test]
fn custom_source_accepts_https_and_normalizes_trailing_slash() {
    let source = source_for("https://status.example.com/team")
        .unwrap()
        .unwrap();
    assert_eq!(source.name, "status.example.com");
    assert_eq!(source.page_url.as_str(), "https://status.example.com/team/");
    assert_eq!(
        source.feed_url.as_str(),
        "https://status.example.com/team/api/v2/summary.json"
    );
    assert_eq!(source.format, FeedFormat::Statuspage);
    assert_eq!(source.components, ComponentFilter::All);

    let same = source_for("https://status.example.com/team/")
        .unwrap()
        .unwrap();
    assert_eq!(same.feed_url, source.feed_url);
}

#[test]
fn custom_source_accepts_literal_loopback_http() {
    let v4 = source_for("http://127.0.0.1:8765").unwrap().unwrap();
    assert_eq!(
        v4.feed_url.as_str(),
        "http://127.0.0.1:8765/api/v2/summary.json"
    );
    assert!(source_for("http://[::1]:8765/").unwrap().is_some());
}

#[test]
fn custom_source_rejects_unsafe_urls() {
    for url in [
        "https://user:pass@status.example.com",
        "https://user@status.example.com",
        "https://status.example.com/?x=1",
        "https://status.example.com/#frag",
        "javascript:alert(1)",
        "http://10.0.0.1",
        "http://169.254.169.254",
        "http://localhost:8765",
        "http://status.example.com",
        "https://",
        "file:///etc/passwd",
        "not a url",
    ] {
        assert!(
            matches!(source_for(url), Err(PluginError::InvalidConfig(_))),
            "{url} should be rejected"
        );
    }
}

#[test]
fn deployment_health_orders_replica_counts_before_conditions() {
    let level = |desired, available, unavailable| {
        deployment_health("c", desired, available, unavailable).map(|c| c.level)
    };
    assert_eq!(
        level(Some(0), Some(1), false),
        Some(HealthLevel::MajorOutage)
    );
    assert_eq!(
        level(Some(2), Some(0), false),
        Some(HealthLevel::MajorOutage)
    );
    assert_eq!(level(Some(3), Some(2), true), Some(HealthLevel::Degraded));
    assert_eq!(level(Some(2), Some(2), true), None);
    assert_eq!(level(None, None, true), Some(HealthLevel::MajorOutage));
    assert_eq!(level(None, None, false), None);
}
