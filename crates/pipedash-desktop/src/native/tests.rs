use serde_json::json;

use super::*;

fn link(raw: &str) -> Option<NativeIntent> {
    parse_deep_link(&Url::parse(raw).unwrap())
}

fn to(path: &str, run_number: Option<u64>) -> Option<NativeIntent> {
    Some(NativeIntent::Link {
        path: path.to_string(),
        run_number,
    })
}

#[test]
fn app_pages_open_from_custom_scheme_links() {
    assert_eq!(link("pipedash://pipelines"), to("/pipelines", None));
    assert_eq!(
        link("pipedash://Notifications/"),
        to("/notifications", None)
    );
    assert_eq!(link("pipedash://providers"), to("/providers", None));
    assert_eq!(link("pipedash://settings"), to("/settings", None));
    assert_eq!(
        link("pipedash://settings/app-lock"),
        to("/settings/app-lock", None)
    );
}

#[test]
fn a_pipeline_link_keeps_its_encoded_id_and_an_optional_run() {
    assert_eq!(
        link("pipedash://pipelines/stub__1__acme%2Fapp%2Fci.yml"),
        to("/pipelines/stub__1__acme%2Fapp%2Fci.yml", None)
    );
    assert_eq!(
        link("pipedash://pipelines/stub__2__build/runs/41"),
        to("/pipelines/stub__2__build", Some(41))
    );
}

#[test]
fn a_pairing_link_opens_the_connect_screen() {
    let raw = "pipedash://pair?url=http%3A%2F%2F192.168.1.20%3A8080&code=ABCDEFGH";
    let intent = link(raw).unwrap();

    assert_eq!(
        intent,
        NativeIntent::Pair {
            link: raw.to_string()
        }
    );
    assert_eq!(
        serde_json::to_value(intent).unwrap(),
        json!({ "type": "pair", "link": raw })
    );
}

#[test]
fn links_the_app_cannot_route_are_ignored() {
    for raw in [
        "https://pipelines/abc",
        "pipedash://unknown",
        "pipedash://pipelines/a/b",
        "pipedash://pipelines/bad%zz",
        "pipedash://pipelines/abc/runs/not-a-number",
        "pipedash://pipelines/abc/runs/-1",
        "pipedash://settings/Bad_Section",
        "pipedash://settings/a/b",
        "pipedash://pair",
        "pipedash://pair?url=http%3A%2F%2Fhost",
    ] {
        assert_eq!(link(raw), None, "{raw}");
    }
}

#[test]
fn peeking_does_not_consume_the_pending_intent() {
    let pending = PendingIntent::default();
    assert_eq!(pending.peek(), None);

    let intent = NativeIntent::Link {
        path: "/providers".to_string(),
        run_number: None,
    };
    pending.set(intent.clone());
    let first = pending.peek().unwrap();

    assert_eq!(first.intent, intent);
    assert_eq!(pending.peek(), Some(first));
}

#[test]
fn the_latest_intent_survives_a_stale_acknowledgment() {
    let pending = PendingIntent::default();
    pending.set(NativeIntent::Ui {
        action: NativeUiAction::ManagePins,
    });
    let first = pending.peek().unwrap();
    let latest_intent = NativeIntent::Notification {
        action: "tap".to_string(),
        target: NotificationTarget {
            key: Some("pipeline:p1".to_string()),
            ..Default::default()
        },
    };
    pending.set(latest_intent.clone());
    let latest = pending.peek().unwrap();

    assert!(latest.id > first.id);
    assert_eq!(latest.intent, latest_intent);
    pending.ack(first.id);
    assert_eq!(pending.peek(), Some(latest.clone()));
    pending.ack(latest.id);
    assert_eq!(pending.peek(), None);
}

#[test]
fn pending_intent_ids_increase_after_acknowledgment() {
    let pending = PendingIntent::default();
    let intent = NativeIntent::Ui {
        action: NativeUiAction::ManageViews,
    };
    pending.set(intent.clone());
    let first = pending.peek().unwrap();
    pending.ack(first.id);
    pending.set(intent);

    assert!(pending.peek().unwrap().id > first.id);
}

#[test]
fn ui_intents_serialize_camel_case_actions_in_pending_envelopes() {
    for (action, serialized) in [
        (NativeUiAction::ManagePins, "managePins"),
        (NativeUiAction::ManageViews, "manageViews"),
    ] {
        let pending = PendingNativeIntent {
            id: 4,
            intent: NativeIntent::Ui { action },
        };

        assert_eq!(
            serde_json::to_value(pending).unwrap(),
            json!({ "id": 4, "intent": { "type": "ui", "action": serialized } })
        );
    }
}

#[test]
fn intents_serialize_flat_with_a_type_tag() {
    let notification = NativeIntent::Notification {
        action: "open-run".to_string(),
        target: NotificationTarget {
            key: Some("pipeline:p1".to_string()),
            pipeline_id: Some("p1".to_string()),
            provider_id: Some(3),
            run_number: Some(12),
        },
    };
    assert_eq!(
        serde_json::to_value(notification).unwrap(),
        json!({
            "type": "notification",
            "action": "open-run",
            "key": "pipeline:p1",
            "pipelineId": "p1",
            "providerId": 3,
            "runNumber": 12,
        })
    );
    assert_eq!(
        serde_json::to_value(to("/pipelines", None).unwrap()).unwrap(),
        json!({ "type": "link", "path": "/pipelines" })
    );
}

#[test]
fn a_local_notification_target_survives_the_extra_round_trip() {
    let target = NotificationTarget {
        key: Some("pipeline:p1".to_string()),
        pipeline_id: Some("p1".to_string()),
        provider_id: Some(3),
        run_number: None,
    };
    let extra: HashMap<String, Value> = target
        .extra()
        .into_iter()
        .map(|(name, value)| (name.to_string(), Value::String(value)))
        .collect();

    assert_eq!(extra.len(), 3);
    assert_eq!(NotificationTarget::from_extra(&extra), target);
}

#[test]
fn a_native_response_parses_numbers_and_drops_bad_ones() {
    let response = NotificationResponse {
        action_id: "tap".to_string(),
        key: Some("k".to_string()),
        pipeline_id: Some("p".to_string()),
        provider_id: Some("4".to_string()),
        run_number: Some("seven".to_string()),
    };

    assert_eq!(
        NotificationTarget::from_response(&response),
        NotificationTarget {
            key: Some("k".to_string()),
            pipeline_id: Some("p".to_string()),
            provider_id: Some(4),
            run_number: None,
        }
    );
}

#[test]
fn the_privacy_cover_follows_the_app_lock_settings() {
    let cover = privacy_cover_for(AppLockSettings {
        enabled: true,
        lock_on_background: false,
    });
    assert!(cover.armed);
    assert!(!cover.hold_for_frontend);
    assert!(!privacy_cover_for(AppLockSettings::default()).armed);
}
