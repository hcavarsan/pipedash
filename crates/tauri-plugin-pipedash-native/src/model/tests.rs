use serde_json::json;

use super::*;

#[test]
fn haptic_kinds_and_appearances_use_the_lowercase_names_the_frontend_sends() {
    assert_eq!(
        serde_json::from_value::<HapticKind>(json!("selection")).unwrap(),
        HapticKind::Selection
    );
    assert_eq!(
        serde_json::from_value::<HapticKind>(json!("rigid")).unwrap(),
        HapticKind::Rigid
    );
    assert!(serde_json::from_value::<HapticKind>(json!("buzz")).is_err());
    assert_eq!(
        serde_json::from_value::<Appearance>(json!("system")).unwrap(),
        Appearance::System
    );
    assert!(serde_json::from_value::<Appearance>(json!("auto")).is_err());
}

#[test]
fn accessibility_flags_serialize_in_camel_case() {
    assert_eq!(
        serde_json::to_value(Accessibility {
            reduce_transparency: true,
            increase_contrast: false,
        })
        .unwrap(),
        json!({ "reduceTransparency": true, "increaseContrast": false })
    );
}

#[test]
fn privacy_cover_arguments_match_the_swift_decoder() {
    assert_eq!(
        serde_json::to_value(PrivacyCover {
            armed: true,
            hold_for_frontend: false,
        })
        .unwrap(),
        json!({ "armed": true, "holdForFrontend": false })
    );
}

#[test]
fn a_notification_response_tolerates_missing_fields() {
    let response: NotificationResponse = serde_json::from_value(json!({
        "actionId": "open-run",
        "pipelineId": "stub__1__a/b/c.yml",
        "runNumber": "7",
    }))
    .unwrap();

    assert_eq!(response.action_id, "open-run");
    assert_eq!(response.pipeline_id.as_deref(), Some("stub__1__a/b/c.yml"));
    assert_eq!(response.run_number.as_deref(), Some("7"));
    assert_eq!(response.key, None);
    assert_eq!(response.provider_id, None);
}

#[test]
fn local_notification_settings_use_camel_case_and_tolerate_a_missing_reason() {
    let settings: LocalNotificationSettings =
        serde_json::from_value(json!({ "enabled": true, "available": false })).unwrap();
    assert_eq!(settings.reason, None);
    assert_eq!(
        serde_json::to_value(&settings).unwrap(),
        json!({ "enabled": true, "available": false, "reason": null })
    );
}

#[test]
fn notification_refresh_events_match_the_swift_payload() {
    let refresh: NotificationRefresh =
        serde_json::from_value(json!({ "id": "task-1", "expired": true })).unwrap();
    assert_eq!(
        refresh,
        NotificationRefresh {
            id: "task-1".to_string(),
            expired: true,
        }
    );
    assert!(serde_json::from_value::<NotificationRefresh>(json!({ "id": "task-1" })).is_err());
}
