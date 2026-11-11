use std::sync::atomic::Ordering;

use chrono::{
    Duration as Span,
    Utc,
};
use pipedash_core::application::{
    LocalNotification,
    ObservedNotification,
};
use pipedash_core::domain::{
    AlertKind,
    NotificationPreferences,
    OnceWatch,
    RunFinish,
};

use super::preferences::{
    parse_answer,
    SavedPreferences,
    ServerPreferences,
};
use super::*;

fn observed(
    id: u64, kind: AlertKind, pipeline_id: &str, finish: Option<RunFinish>,
) -> ObservedNotification {
    ObservedNotification {
        id,
        at: Utc::now(),
        notice: LocalNotification {
            key: format!("key:{id}"),
            title: "title".to_owned(),
            body: "body".to_owned(),
            pipeline_id: Some(pipeline_id.to_owned()),
            provider_id: Some(1),
            provider_ids: vec![1],
            kind,
            finish,
            level: None,
            link: None,
        },
    }
}

#[test]
fn disabled_notifications_cannot_start_refresh_and_old_expiration_cannot_cancel_new_task() {
    let state = LocalNotificationsState::default();
    assert!(state.start_refresh("old").is_none());
    state.enabled.store(true, Ordering::Release);
    let old = state.start_refresh("old").unwrap();
    assert!(state.start_refresh("duplicate").is_none());
    state.cancel_refresh(Some("old"));
    assert!(*old.borrow());
    let current = state.start_refresh("new").unwrap();
    state.cancel_refresh(Some("old"));
    assert!(!*current.borrow());
    state.enabled.store(false, Ordering::Release);
    state.invalidate();
    assert!(*current.borrow());
    assert!(state.start_refresh("disabled").is_none());
}

#[test]
fn the_app_copy_decides_what_is_shown_and_uses_a_one_time_watch_once() {
    let state = LocalNotificationsState::default();
    let request = OnceWatch {
        pipeline_id: "stub__1__api".to_owned(),
        since: Utc::now(),
        fired_at: None,
    };
    let chosen = NotificationPreferences {
        failed: false,
        watch_once: vec![request.clone()],
        ..NotificationPreferences::default()
    };
    assert!(state.merge_preferences(&chosen).1);

    assert!(state
        .admit(&observed(1, AlertKind::Failed, "stub__1__web", None))
        .is_none());

    let finish = RunFinish {
        at: request.since + Span::minutes(1),
        previous: None,
    };
    let shown = state
        .admit(&observed(
            2,
            AlertKind::Finished,
            "stub__1__api",
            Some(finish),
        ))
        .unwrap();
    let used = OnceWatch {
        fired_at: Some(finish.at),
        ..request
    };
    assert_eq!(shown.fired, Some(used.clone()));

    let (resent, changed) = state.merge_preferences(&chosen);
    assert!(!changed);
    assert_eq!(resent.fired_watches(), [used]);
}

#[test]
fn the_server_answer_lists_used_watches_and_anything_else_is_a_failed_send() {
    let body = r#"{"fired":[{"pipeline_id":"stub__1__api","since":"2026-11-03T00:00:00Z","fired_at":"2026-11-03T00:05:00Z"}]}"#;

    let saved = parse_answer::<SavedPreferences>(200, body).unwrap();
    assert_eq!(saved.fired.len(), 1);
    assert_eq!(saved.fired[0].pipeline_id, "stub__1__api");
    assert!(saved.fired[0].fired_at.is_some());
    assert!(parse_answer::<SavedPreferences>(500, body).is_none());
    assert!(parse_answer::<SavedPreferences>(200, r#"{"fired":["stub__1__api"]}"#).is_none());
    assert!(
        parse_answer::<ServerPreferences>(200, r#"{"preferences":null}"#)
            .unwrap()
            .preferences
            .is_none()
    );
}
