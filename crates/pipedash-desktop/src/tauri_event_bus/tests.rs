use chrono::Utc;
use pipedash_core::application::{
    LocalNotification,
    ObservedNotification,
};
use pipedash_core::domain::{
    AlertKind,
    NotificationPreferences,
};

use super::*;

fn observed(kind: AlertKind) -> CoreEvent {
    CoreEvent::NotificationObserved {
        notification: ObservedNotification {
            id: 7,
            at: Utc::now(),
            notice: LocalNotification {
                key: "pipeline:stub__1__api".to_owned(),
                title: "api failed".to_owned(),
                body: "main".to_owned(),
                pipeline_id: Some("stub__1__api".to_owned()),
                provider_id: Some(1),
                provider_ids: vec![1],
                kind,
                finish: None,
                level: None,
                link: None,
            },
        },
    }
}

#[test]
fn an_observed_notification_reaches_the_webview_only_as_what_the_app_copy_admits() {
    let state = LocalNotificationsState::default();
    let event = observed(AlertKind::Failed);

    let shown = admit(&state, &event);
    let (name, payload) = webview_payload(&event, shown.as_ref()).unwrap();
    assert_eq!(name, "notification");
    assert_eq!(payload["id"], 7);
    assert_eq!(payload["key"], "pipeline:stub__1__api");
    assert_eq!(payload["tone"], "danger");
    assert!(payload["fired"].is_null());

    state.merge_preferences(&NotificationPreferences {
        failed: false,
        ..NotificationPreferences::default()
    });
    let shown = admit(&state, &event);
    assert!(shown.is_none());
    assert!(webview_payload(&event, shown.as_ref()).is_none());
}

#[test]
fn other_events_keep_their_own_name_and_payload() {
    let event = CoreEvent::ProvidersChanged;

    let (name, payload) = webview_payload(&event, None).unwrap();

    assert_eq!(name, event.event_name());
    assert_eq!(payload, event.to_json_payload());
}
