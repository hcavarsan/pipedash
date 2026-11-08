use super::*;

#[test]
fn kinds_round_trip_through_their_wire_names() {
    for kind in [
        AlertKind::Failed,
        AlertKind::Degraded,
        AlertKind::LongRun,
        AlertKind::Outage,
        AlertKind::Recovered,
        AlertKind::Finished,
        AlertKind::Test,
    ] {
        assert_eq!(AlertKind::parse(kind.as_str()), Some(kind));
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            serde_json::json!(kind.as_str())
        );
    }
    assert_eq!(AlertKind::parse("nope"), None);
}

#[test]
fn tones_group_kinds_by_severity() {
    assert_eq!(AlertKind::Failed.tone(), "danger");
    assert_eq!(AlertKind::Degraded.tone(), "danger");
    assert_eq!(AlertKind::LongRun.tone(), "warning");
    assert_eq!(AlertKind::Outage.tone(), "warning");
    assert_eq!(AlertKind::Recovered.tone(), "success");
    assert_eq!(AlertKind::Test.tone(), "success");
}

#[test]
fn preferences_filter_by_kind_provider_and_pipeline() {
    let preferences = NotificationPreferences {
        long_run: false,
        muted_providers: vec![2],
        muted_pipelines: vec!["stub__1__api".into()],
        ..NotificationPreferences::default()
    };

    assert!(preferences.allows(AlertKind::Failed, &[1], Some("stub__1__web"), None));
    assert!(!preferences.allows(AlertKind::LongRun, &[1], Some("stub__1__web"), None));
    assert!(!preferences.allows(AlertKind::Failed, &[1], Some("stub__1__api"), None));
    assert!(!preferences.allows(AlertKind::Outage, &[2], None, None));
    assert!(preferences.allows(AlertKind::Outage, &[2, 3], None, None));
    assert!(preferences.allows(AlertKind::Outage, &[3], None, None));
    assert!(preferences.allows(AlertKind::Outage, &[], None, None));
    assert!(preferences.allows(AlertKind::Test, &[2], Some("stub__1__api"), None));
    assert!(preferences
        .kinds_only()
        .allows(AlertKind::Failed, &[2], Some("stub__1__api"), None));
}

#[test]
fn a_watched_pipeline_beats_provider_mutes_and_switches_but_not_its_own_mute() {
    let preferences = NotificationPreferences {
        failed: false,
        long_run: false,
        muted_providers: vec![2],
        muted_pipelines: vec!["stub__2__muted".into()],
        watched_pipelines: vec!["stub__2__api".into(), "stub__2__muted".into()],
        ..NotificationPreferences::default()
    };
    let watched = Some("stub__2__api");

    for kind in [AlertKind::Failed, AlertKind::LongRun, AlertKind::Finished] {
        assert!(preferences.allows(kind, &[2], watched, None), "{kind:?}");
    }
    assert!(!preferences.allows(AlertKind::Finished, &[1], Some("stub__1__web"), None));
    assert!(!preferences.allows(AlertKind::Finished, &[], None, None));
    assert!(!preferences.allows(AlertKind::Finished, &[2], Some("stub__2__muted"), None));
    assert!(!preferences
        .kinds_only()
        .allows(AlertKind::Finished, &[2], watched, None));
}

#[test]
fn a_one_time_watch_lets_only_the_first_finish_after_it_through() {
    let since = DateTime::parse_from_rfc3339("2026-11-04T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let minutes = |m: i64| since + chrono::Duration::minutes(m);
    let preferences = NotificationPreferences {
        failed: false,
        muted_providers: vec![1],
        watch_once: vec![OnceWatch {
            pipeline_id: "stub__1__deploy".into(),
            since,
            fired_at: None,
        }],
        ..NotificationPreferences::default()
    };
    let allows = |kind, finish: RunFinish| {
        preferences.allows(kind, &[1], Some("stub__1__deploy"), Some(&finish))
    };

    assert!(allows(
        AlertKind::Finished,
        RunFinish {
            at: minutes(5),
            previous: Some(minutes(-30)),
        }
    ));
    assert!(allows(
        AlertKind::Failed,
        RunFinish {
            at: minutes(5),
            previous: None,
        }
    ));
    assert!(!allows(
        AlertKind::Finished,
        RunFinish {
            at: minutes(9),
            previous: Some(minutes(5)),
        }
    ));
    assert!(!allows(
        AlertKind::Finished,
        RunFinish {
            at: minutes(-1),
            previous: None,
        }
    ));
    assert!(!preferences.allows(AlertKind::Finished, &[1], Some("stub__1__deploy"), None));
    assert!(!preferences.allows(
        AlertKind::Finished,
        &[1],
        Some("stub__1__other"),
        Some(&RunFinish {
            at: minutes(5),
            previous: None,
        })
    ));
}

#[test]
fn a_used_one_time_watch_only_lets_its_own_finish_through_even_after_a_restart() {
    let since = DateTime::parse_from_rfc3339("2026-11-04T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let minutes = |m: i64| since + chrono::Duration::minutes(m);
    let request = OnceWatch {
        pipeline_id: "stub__1__deploy".into(),
        since,
        fired_at: None,
    };
    let mut stored = NotificationPreferences {
        watch_once: vec![request.clone()],
        ..NotificationPreferences::default()
    };
    let first = RunFinish {
        at: minutes(5),
        previous: None,
    };
    let after_restart = RunFinish {
        at: minutes(40),
        previous: None,
    };

    assert_eq!(stored.fire_once("stub__1__deploy", &first).len(), 1);
    assert!(stored
        .fire_once("stub__1__deploy", &after_restart)
        .is_empty());

    let mut resent = NotificationPreferences {
        watch_once: vec![
            request,
            OnceWatch {
                pipeline_id: "stub__1__deploy".into(),
                since: minutes(50),
                fired_at: None,
            },
        ],
        ..NotificationPreferences::default()
    };
    resent.keep_fired(&stored);
    let allows = |finish: &RunFinish| {
        resent.watch_once[..1]
            .iter()
            .any(|once| once.matches("stub__1__deploy", finish))
    };

    assert!(allows(&first));
    assert!(!allows(&after_restart));
    assert_eq!(resent.watch_once[1].fired_at, None);
}

#[test]
fn missing_preference_fields_keep_their_defaults() {
    let preferences: NotificationPreferences =
        serde_json::from_value(serde_json::json!({ "outage": false, "future": true })).unwrap();

    assert_eq!(
        preferences,
        NotificationPreferences {
            outage: false,
            ..NotificationPreferences::default()
        }
    );
}

#[test]
fn alert_ids_are_32_hex_characters() {
    let id = new_alert_id();
    assert_eq!(id.len(), 32);
    assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(id, new_alert_id());
}
