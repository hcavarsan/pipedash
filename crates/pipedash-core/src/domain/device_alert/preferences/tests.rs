use chrono::{
    DateTime,
    Duration,
    TimeZone,
    Utc,
};

use super::*;

const PIPELINE: &str = "stub__1__acme/api__ci.yml";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap()
}

fn apply(preferences: &mut NotificationPreferences, change: PreferenceChange) {
    change_preferences(preferences, change, now()).unwrap();
}

fn muted(muted: bool) -> PreferenceChange {
    PreferenceChange::PipelineMuted {
        pipeline_id: PIPELINE.into(),
        muted,
    }
}

fn watched(watched: bool) -> PreferenceChange {
    PreferenceChange::PipelineWatched {
        pipeline_id: PIPELINE.into(),
        watched,
    }
}

fn watch_next() -> PreferenceChange {
    PreferenceChange::WatchNextFinish {
        pipeline_id: PIPELINE.into(),
    }
}

fn once(pipeline_id: &str, since: DateTime<Utc>) -> OnceWatch {
    OnceWatch {
        pipeline_id: pipeline_id.into(),
        since,
        fired_at: None,
    }
}

fn pending(preferences: &NotificationPreferences) -> Vec<&OnceWatch> {
    preferences
        .watch_once
        .iter()
        .filter(|watch| watch.fired_at.is_none())
        .collect()
}

#[test]
fn watching_and_muting_a_pipeline_replace_each_other_and_a_pending_request_goes_with_either() {
    let mut preferences = NotificationPreferences::default();

    apply(&mut preferences, muted(true));
    apply(&mut preferences, watch_next());
    assert!(preferences.muted_pipelines.is_empty());
    assert_eq!(preferences.watch_once.len(), 1);

    apply(&mut preferences, muted(true));
    assert_eq!(preferences.muted_pipelines, [PIPELINE]);
    assert!(preferences.watch_once.is_empty());

    apply(&mut preferences, watched(true));
    assert!(preferences.muted_pipelines.is_empty());
    assert_eq!(preferences.watched_pipelines, [PIPELINE]);

    apply(&mut preferences, muted(true));
    assert!(preferences.watched_pipelines.is_empty());
    assert_eq!(preferences.muted_pipelines, [PIPELINE]);

    apply(&mut preferences, muted(false));
    assert!(preferences.muted_pipelines.is_empty());
}

#[test]
fn watching_clears_a_pending_one_time_request() {
    let mut preferences = NotificationPreferences::default();

    apply(&mut preferences, watch_next());
    apply(&mut preferences, watched(true));
    assert!(preferences.watch_once.is_empty());
    assert_eq!(preferences.watched_pipelines, [PIPELINE]);

    apply(&mut preferences, watched(false));
    assert!(preferences.watched_pipelines.is_empty());
}

#[test]
fn asking_again_replaces_the_pending_request_and_cancelling_drops_it() {
    let earlier = now() - Duration::days(1);
    let mut preferences = NotificationPreferences {
        watch_once: vec![once(PIPELINE, earlier), once("other", earlier)],
        ..NotificationPreferences::default()
    };

    apply(&mut preferences, watch_next());
    let mine: Vec<_> = preferences
        .watch_once
        .iter()
        .filter(|watch| watch.pipeline_id == PIPELINE)
        .collect();
    assert_eq!(mine.len(), 1);
    assert!(mine[0].since > earlier);
    assert_eq!(preferences.watch_once.len(), 2);

    apply(
        &mut preferences,
        PreferenceChange::CancelNextFinish {
            pipeline_id: PIPELINE.into(),
        },
    );
    let remaining: Vec<_> = preferences
        .watch_once
        .iter()
        .map(|watch| watch.pipeline_id.as_str())
        .collect();
    assert_eq!(remaining, ["other"]);

    apply(
        &mut preferences,
        PreferenceChange::CancelNextFinish {
            pipeline_id: "other".into(),
        },
    );
    assert!(preferences.watch_once.is_empty());
}

#[test]
fn asking_again_after_a_request_fired_makes_a_new_pending_one() {
    let mut preferences = NotificationPreferences {
        watch_once: vec![OnceWatch {
            fired_at: Some(now() - Duration::minutes(30)),
            ..once(PIPELINE, now() - Duration::hours(1))
        }],
        ..NotificationPreferences::default()
    };

    apply(&mut preferences, watch_next());

    assert_eq!(pending(&preferences).len(), 1);
    assert_eq!(preferences.watch_once.len(), 1);
}

#[test]
fn kinds_and_provider_mutes_toggle_and_only_real_kinds_are_accepted() {
    let mut preferences = NotificationPreferences::default();

    apply(
        &mut preferences,
        PreferenceChange::Kind {
            kind: AlertKind::LongRun,
            enabled: false,
        },
    );
    assert!(!preferences.long_run);
    assert!(preferences.failed);

    apply(
        &mut preferences,
        PreferenceChange::ProviderMuted {
            provider_id: 3,
            muted: true,
        },
    );
    apply(
        &mut preferences,
        PreferenceChange::ProviderMuted {
            provider_id: 3,
            muted: true,
        },
    );
    assert_eq!(preferences.muted_providers, [3]);
    apply(
        &mut preferences,
        PreferenceChange::ProviderMuted {
            provider_id: 3,
            muted: false,
        },
    );
    assert!(preferences.muted_providers.is_empty());

    let rejected = change_preferences(
        &mut preferences,
        PreferenceChange::Kind {
            kind: AlertKind::Test,
            enabled: false,
        },
        now(),
    );
    assert!(matches!(rejected, Err(DomainError::InvalidConfig(_))));
}

#[test]
fn one_time_requests_older_than_a_week_are_dropped_and_newer_ones_kept() {
    let mut preferences = NotificationPreferences {
        watch_once: vec![
            once("stale", now() - Duration::days(8)),
            once("recent", now() - Duration::days(6)),
        ],
        ..NotificationPreferences::default()
    };

    assert!(preferences.drop_stale_watches(now()));
    assert!(!preferences.drop_stale_watches(now()));

    let kept: Vec<_> = preferences
        .watch_once
        .iter()
        .map(|watch| watch.pipeline_id.as_str())
        .collect();
    assert_eq!(kept, ["recent"]);
}

#[test]
fn a_change_also_drops_stale_requests() {
    let mut preferences = NotificationPreferences {
        watch_once: vec![once("stale", now() - Duration::days(9))],
        ..NotificationPreferences::default()
    };

    apply(&mut preferences, watch_next());

    assert_eq!(pending(&preferences).len(), 1);
    assert_eq!(preferences.watch_once[0].pipeline_id, PIPELINE);
}

#[test]
fn the_snapshot_ranks_muted_over_watching_over_pending() {
    let preferences = NotificationPreferences {
        muted_pipelines: vec!["a".into()],
        watched_pipelines: vec!["a".into(), "b".into()],
        watch_once: vec![
            once("a", now()),
            once("b", now()),
            once("c", now()),
            OnceWatch {
                fired_at: Some(now()),
                ..once("d", now())
            },
        ],
        ..NotificationPreferences::default()
    };

    let snapshot = NotificationPreferenceSnapshot::new(preferences.clone());

    assert_eq!(snapshot.preferences, preferences);
    assert_eq!(
        snapshot.pipeline_states,
        [
            ("a".to_owned(), PipelineNotifyState::Muted),
            ("b".to_owned(), PipelineNotifyState::Watching),
            ("c".to_owned(), PipelineNotifyState::Pending),
        ]
        .into()
    );
}

#[test]
fn merging_keeps_the_first_fired_mark_and_reports_whether_anything_changed() {
    let watch = once(PIPELINE, now());
    let fired = OnceWatch {
        fired_at: Some(now() + Duration::minutes(30)),
        ..watch.clone()
    };
    let mut stored = NotificationPreferences {
        watch_once: vec![fired.clone()],
        ..NotificationPreferences::default()
    };
    let resent = NotificationPreferences {
        watch_once: vec![watch.clone()],
        ..NotificationPreferences::default()
    };

    assert!(!stored.merge(&resent));
    assert_eq!(stored.watch_once, std::slice::from_ref(&fired));

    let other = NotificationPreferences {
        failed: false,
        ..resent
    };
    assert!(stored.merge(&other));
    assert!(!stored.failed);
    assert_eq!(stored.fired_watches(), [fired]);
}

#[test]
fn fired_watches_known_locally_and_by_the_server_are_listed_once() {
    let later = now() + Duration::hours(1);

    let merged = merge_fired_watches(
        vec![once("b", now()), once("a", now())],
        vec![once("a", now()), once("a", later)],
    );

    assert_eq!(
        merged,
        [once("a", now()), once("a", later), once("b", now())]
    );
}

#[test]
fn the_wire_format_is_tagged_and_snake_case() {
    let change: PreferenceChange = serde_json::from_value(serde_json::json!({
        "type": "pipeline_watched",
        "pipeline_id": "p",
        "watched": true,
    }))
    .unwrap();

    assert_eq!(
        change,
        PreferenceChange::PipelineWatched {
            pipeline_id: "p".into(),
            watched: true,
        }
    );
    assert_eq!(
        serde_json::to_value(PipelineNotifyState::Pending).unwrap(),
        "pending"
    );
}
