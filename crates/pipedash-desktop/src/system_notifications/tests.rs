use super::*;

#[cfg(mobile)]
#[test]
fn only_notifications_about_a_pipeline_get_the_open_run_category() {
    let pipeline = NotificationTarget {
        key: Some("pipeline:p1".to_string()),
        pipeline_id: Some("p1".to_string()),
        ..Default::default()
    };
    let provider = NotificationTarget {
        key: Some("provider:3".to_string()),
        ..Default::default()
    };

    assert_eq!(category_for(&pipeline), Some(FAILURE_CATEGORY));
    assert_eq!(category_for(&provider), None);
}

#[cfg(not(target_os = "ios"))]
mod clicks {
    use serde_json::{
        json,
        Value,
    };

    use super::*;

    fn performed(action_id: &str, notification: Value) -> ActionPerformed {
        serde_json::from_value(json!({
            "actionId": action_id,
            "inputValue": null,
            "notification": notification,
        }))
        .unwrap()
    }

    #[test]
    fn a_click_on_a_notification_reports_its_target() {
        let tapped = performed(
            "tap",
            json!({
                "id": 7,
                "title": "build failed",
                "extra": {
                    "key": "pipeline:p1",
                    "pipelineId": "p1",
                    "providerId": "3",
                    "runNumber": "42"
                }
            }),
        );

        assert_eq!(
            clicked_target(&tapped),
            Some(NotificationTarget {
                key: Some("pipeline:p1".to_string()),
                pipeline_id: Some("p1".to_string()),
                provider_id: Some(3),
                run_number: Some(42),
            })
        );
    }

    #[cfg(desktop)]
    #[test]
    fn desktop_actions_accept_provider_and_test_notification_targets() {
        for (extra, target) in [
            (
                json!({ "key": "provider:3", "providerId": "3" }),
                NotificationTarget {
                    key: Some("provider:3".to_string()),
                    provider_id: Some(3),
                    ..Default::default()
                },
            ),
            (
                json!({ "key": "desktop-test" }),
                NotificationTarget {
                    key: Some("desktop-test".to_string()),
                    ..Default::default()
                },
            ),
        ] {
            for action in ["tap", "open"] {
                let clicked = performed(action, json!({ "id": 7, "extra": extra.clone() }));

                assert_eq!(clicked_target(&clicked), Some(target.clone()));
            }
        }
    }

    #[cfg(target_os = "android")]
    #[test]
    fn android_does_not_accept_the_desktop_open_action() {
        let opened = performed(
            "open",
            json!({ "id": 7, "extra": { "key": "pipeline:p1" } }),
        );

        assert_eq!(clicked_target(&opened), None);
    }

    #[test]
    fn other_actions_and_notifications_without_a_target_report_nothing() {
        let with_target = json!({ "id": 7, "extra": { "key": "status-page#3" } });

        for action in ["dismiss", "open-run", "retry", "cancel"] {
            assert_eq!(
                clicked_target(&performed(action, with_target.clone())),
                None
            );
        }
        assert_eq!(clicked_target(&performed("tap", json!({ "id": 7 }))), None);
        assert_eq!(clicked_target(&performed("tap", Value::Null)), None);
    }
}
