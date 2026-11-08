use super::*;

#[test]
fn test_event_names() {
    assert_eq!(
        CoreEvent::ProvidersChanged.event_name(),
        "providers-changed"
    );
    assert_eq!(
        CoreEvent::PipelineViewsChanged.event_name(),
        "pipeline-views-changed"
    );
    assert_eq!(
        CoreEvent::PipelineStatusChanged {
            pipeline_ids: vec!["a".to_string()]
        }
        .event_name(),
        "pipeline-status-changed"
    );
}

#[test]
fn test_event_serialization() {
    let event = CoreEvent::PipelineStatusChanged {
        pipeline_ids: vec!["stub__1__web".to_string(), "stub__1__api".to_string()],
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("PipelineStatusChanged"));
    assert!(json.contains("stub__1__web"));
    assert_eq!(
        event.to_json_payload(),
        serde_json::json!({ "pipeline_ids": ["stub__1__web", "stub__1__api"] })
    );
}

#[test]
fn test_provider_health_alert_wire_format() {
    let alert = |link| CoreEvent::ProviderHealthAlert {
        alert_id: "alert-7".to_string(),
        alert_key: "incident:7".to_string(),
        notification_key: "status-page:feed".to_string(),
        provider_ids: vec![1, 2],
        level: HealthLevel::MajorOutage,
        title: "Stub is down".to_string(),
        body: "Investigating".to_string(),
        link,
        recovered: false,
    };
    let with_link = alert(Some(AlertLink {
        url: "https://status.example.test/incidents/abc".to_string(),
        kind: AlertLinkKind::Incident,
    }));

    assert_eq!(with_link.event_name(), "provider-health-alert");
    assert_eq!(
        with_link.to_json_payload(),
        serde_json::json!({
            "alert_id": "alert-7",
            "notification_key": "status-page:feed",
            "provider_ids": [1, 2],
            "level": "major_outage",
            "title": "Stub is down",
            "body": "Investigating",
            "link": {
                "url": "https://status.example.test/incidents/abc",
                "kind": "incident"
            },
            "recovered": false,
        })
    );
    assert_eq!(
        alert(None).to_json_payload()["link"],
        serde_json::Value::Null
    );
}

#[test]
fn test_vault_lock_and_pipelines_failed_wire_format() {
    assert_eq!(CoreEvent::VaultLocked.event_name(), "vault-locked");
    assert_eq!(
        CoreEvent::VaultLocked.to_json_payload(),
        serde_json::json!({})
    );

    let failed = CoreEvent::PipelinesFailed {
        pipelines: Vec::new(),
    };
    assert_eq!(failed.event_name(), "pipelines-failed");
    assert_eq!(
        failed.to_json_payload(),
        serde_json::json!({ "pipelines": [] })
    );

    let paired = CoreEvent::DevicePaired {
        id: "d1".into(),
        name: "Pixel".into(),
    };
    assert_eq!(paired.event_name(), "device-paired");
    assert_eq!(
        paired.to_json_payload(),
        serde_json::json!({ "id": "d1", "name": "Pixel" })
    );

    let revoked = CoreEvent::DeviceRevoked { id: "d1".into() };
    assert_eq!(revoked.event_name(), "device-revoked");
    assert_eq!(revoked.to_json_payload(), serde_json::json!({ "id": "d1" }));

    let recorded = CoreEvent::AlertRecorded {
        id: "a1".into(),
        seq: 7,
    };
    assert_eq!(recorded.event_name(), "alert-recorded");
    assert_eq!(
        recorded.to_json_payload(),
        serde_json::json!({ "id": "a1", "seq": 7 })
    );
}
