use serde_json::json;

use super::*;
use crate::event::AlertLinkKind;

fn notice(key: &str, title: &str) -> CenterNotification {
    CenterNotification {
        key: key.to_string(),
        at: 1,
        read: false,
        tone: CenterTone::Warning,
        level: Some(HealthLevel::MajorOutage),
        title: title.to_string(),
        body: "Investigating".to_string(),
        provider_ids: vec![1],
        pipeline_id: None,
        link: None,
        run_number: None,
    }
}

fn at(key: &str, at: i64) -> CenterNotification {
    CenterNotification {
        at,
        ..notice(key, key)
    }
}

fn apply(
    snapshot: NotificationCenterSnapshot, change: NotificationCenterChange,
) -> NotificationCenterSnapshot {
    update_notification_center(snapshot, change).unwrap()
}

fn push(
    snapshot: NotificationCenterSnapshot, notice: CenterNotification,
) -> NotificationCenterSnapshot {
    apply(snapshot, NotificationCenterChange::Push { notice })
}

fn read(snapshot: NotificationCenterSnapshot, key: Option<&str>) -> NotificationCenterSnapshot {
    apply(
        snapshot,
        NotificationCenterChange::Read {
            key: key.map(str::to_string),
        },
    )
}

fn titles(snapshot: &NotificationCenterSnapshot) -> Vec<&str> {
    snapshot
        .items
        .iter()
        .map(|item| item.title.as_str())
        .collect()
}

fn keys(snapshot: &NotificationCenterSnapshot) -> Vec<&str> {
    snapshot
        .items
        .iter()
        .map(|item| item.key.as_str())
        .collect()
}

fn shown(kind: AlertKind) -> ShownNotification {
    ShownNotification {
        id: 3,
        at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        tone: kind.tone(),
        notice: LocalNotification {
            key: "provider:1".to_string(),
            title: "Provider health".to_string(),
            body: "API down".to_string(),
            pipeline_id: None,
            provider_id: Some(1),
            provider_ids: vec![1, 2],
            kind,
            finish: None,
            level: Some(HealthLevel::MajorOutage),
            link: Some(AlertLink {
                url: "https://status.example.com".to_string(),
                kind: AlertLinkKind::Incident,
            }),
        },
        fired: None,
    }
}

#[test]
fn a_notification_with_the_same_key_replaces_the_earlier_one_and_moves_to_the_top() {
    let mut snapshot = push(
        Default::default(),
        notice("status-page:stub", "Actions is down"),
    );
    snapshot = push(snapshot, notice("connection:2", "Can't reach Stub"));
    snapshot = push(
        snapshot,
        CenterNotification {
            tone: CenterTone::Success,
            ..notice("status-page:stub", "Stub is back to normal")
        },
    );

    assert_eq!(
        titles(&snapshot),
        ["Stub is back to normal", "Can't reach Stub"]
    );
    assert!(!snapshot.items[0].read);
    assert_eq!(snapshot.unread, 2);
}

#[test]
fn reading_a_replaced_notification_marks_the_current_entry_for_its_key() {
    let mut snapshot = push(
        Default::default(),
        notice("status-page:stub", "Actions is down"),
    );
    snapshot = push(snapshot, notice("connection:2", "Actions is down"));
    snapshot = push(
        snapshot,
        notice("status-page:stub", "Stub is back to normal"),
    );

    snapshot = read(snapshot, Some("status-page:stub"));

    let state: Vec<_> = snapshot
        .items
        .iter()
        .map(|item| (item.title.as_str(), item.read))
        .collect();
    assert_eq!(
        state,
        [("Stub is back to normal", true), ("Actions is down", false)]
    );
    assert_eq!(snapshot.unread, 1);
}

#[test]
fn mark_all_read_keeps_the_items() {
    let mut snapshot = push(Default::default(), notice("status-page:stub", "a"));
    snapshot = push(snapshot, notice("connection:2", "b"));

    snapshot = read(snapshot, None);

    assert_eq!(snapshot.items.len(), 2);
    assert!(snapshot.items.iter().all(|item| item.read));
    assert_eq!(snapshot.unread, 0);
}

#[test]
fn the_list_keeps_only_the_newest_notifications() {
    let mut snapshot = NotificationCenterSnapshot::default();
    for index in 0..=MAX_CENTER_NOTIFICATIONS {
        snapshot = push(
            snapshot,
            notice(&format!("pipeline:{index}"), &format!("run {index}")),
        );
    }

    assert_eq!(snapshot.items.len(), MAX_CENTER_NOTIFICATIONS);
    assert_eq!(
        snapshot.items[0].title,
        format!("run {MAX_CENTER_NOTIFICATIONS}")
    );
    assert_eq!(snapshot.items.last().unwrap().title, "run 1");
    assert_eq!(snapshot.unread, MAX_CENTER_NOTIFICATIONS);
}

#[test]
fn mark_read_only_affects_the_requested_notification_and_unknown_keys_change_nothing() {
    let mut snapshot = push(Default::default(), notice("status-page:stub", "a"));
    snapshot = push(snapshot, notice("connection:2", "b"));

    snapshot = read(snapshot, Some("connection:2"));
    let revision = snapshot.revision;
    snapshot = read(snapshot, Some("missing"));

    let state: Vec<_> = snapshot
        .items
        .iter()
        .map(|item| (item.key.as_str(), item.read))
        .collect();
    assert_eq!(state, [("connection:2", true), ("status-page:stub", false)]);
    assert_eq!(snapshot.revision, revision);
    assert_eq!(snapshot.unread, 1);
}

#[test]
fn clear_empties_the_list_and_every_action_is_safe_on_an_empty_snapshot() {
    let mut snapshot = read(Default::default(), None);
    snapshot = read(snapshot, Some("x"));
    assert!(snapshot.items.is_empty());
    assert_eq!(snapshot.revision, 0);

    snapshot = push(snapshot, notice("status-page:stub", "a"));
    let revision = snapshot.revision;
    snapshot = apply(snapshot, NotificationCenterChange::Clear);

    assert!(snapshot.items.is_empty());
    assert_eq!(snapshot.unread, 0);
    assert!(snapshot.revision > revision);
}

#[test]
fn history_keeps_fifty_recent_keys_and_replaces_read_entries() {
    let mut history = NotificationCenterSnapshot::default();
    for index in 0..60 {
        history.push(at(&index.to_string(), 1));
    }
    assert_eq!(history.items.len(), 50);
    assert_eq!(history.items.last().unwrap().key, "10");
    history.mark_read(Some("11"));
    assert!(
        history
            .items
            .iter()
            .find(|item| item.key == "11")
            .unwrap()
            .read
    );
    assert_eq!(history.unread, 49);
    history.push(at("11", 1));
    assert_eq!(history.items[0].key, "11");
    assert!(!history.items[0].read);
    assert_eq!(
        history.items.iter().filter(|item| item.key == "11").count(),
        1
    );
    assert_eq!(history.unread, 50);
    let revision = history.revision;
    history.mark_read(Some("missing"));
    assert_eq!(history.revision, revision);
    history.clear();
    assert!(history.items.is_empty());
    assert!(history.revision > revision);
}

#[test]
fn missed_notifications_slot_in_by_time_and_never_replace_newer_ones() {
    let mut history = NotificationCenterSnapshot {
        items: vec![at("newest", 300), at("oldest", 100)],
        ..Default::default()
    };

    assert!(history.insert_missed(at("middle", 200)));
    assert_eq!(keys(&history), ["newest", "middle", "oldest"]);
    assert_eq!(history.unread, 3);

    let revision = history.revision;
    assert!(!history.insert_missed(at("middle", 200)));
    assert!(!history.insert_missed(at("middle", 150)));
    assert_eq!(history.revision, revision);

    assert!(history.insert_missed(at("oldest", 250)));
    assert_eq!(
        history
            .items
            .iter()
            .filter(|item| item.key == "oldest")
            .count(),
        1
    );
    assert_eq!(history.items[1].at, 250);
    assert_eq!(history.revision, revision + 1);
}

#[test]
fn a_missed_notification_older_than_a_full_list_is_dropped() {
    let mut history = NotificationCenterSnapshot::default();
    for index in 0..MAX_CENTER_NOTIFICATIONS {
        history.push(at(&index.to_string(), 1_000 + index as i64));
    }
    let revision = history.revision;

    assert!(!history.insert_missed(at("ancient", 1)));
    assert_eq!(history.items.len(), MAX_CENTER_NOTIFICATIONS);
    assert!(history.items.iter().all(|item| item.key != "ancient"));
    assert_eq!(history.revision, revision);

    assert!(history.insert_missed(at("fresh", 5_000)));
    assert_eq!(history.items.len(), MAX_CENTER_NOTIFICATIONS);
    assert_eq!(history.items[0].key, "fresh");
}

#[test]
fn updating_applies_a_missed_insert_and_recomputes_a_wrong_unread_count() {
    let snapshot = NotificationCenterSnapshot {
        revision: 4,
        items: vec![CenterNotification {
            read: true,
            ..at("seen", 200)
        }],
        unread: 9,
    };

    let updated = apply(
        snapshot,
        NotificationCenterChange::InsertMissed {
            notice: at("missed", 100),
        },
    );

    assert_eq!(keys(&updated), ["seen", "missed"]);
    assert_eq!(updated.revision, 5);
    assert_eq!(updated.unread, 1);
}

#[test]
fn updating_rejects_a_snapshot_above_the_item_cap() {
    let snapshot = NotificationCenterSnapshot {
        revision: 0,
        items: (0..=MAX_CENTER_NOTIFICATIONS)
            .map(|index| at(&index.to_string(), 1))
            .collect(),
        unread: 0,
    };

    let error = update_notification_center(snapshot, NotificationCenterChange::Clear).unwrap_err();

    assert!(matches!(error, DomainError::InvalidConfig(_)));
}

#[test]
fn a_revision_at_the_limit_does_not_overflow() {
    let snapshot = NotificationCenterSnapshot {
        revision: u64::MAX,
        ..Default::default()
    };

    assert_eq!(push(snapshot, at("a", 1)).revision, u64::MAX);
}

#[test]
fn changes_and_snapshots_keep_their_wire_shape() {
    let change: NotificationCenterChange = serde_json::from_value(json!({
        "type": "insert_missed",
        "notice": {
            "key": "k", "at": 7, "read": false, "tone": "danger", "level": null,
            "title": "t", "body": "b", "providerIds": [1], "pipelineId": "p",
            "link": { "url": "https://status.example.com", "kind": "status_page" },
        },
    }))
    .unwrap();
    let NotificationCenterChange::InsertMissed { notice } = &change else {
        panic!("expected insert_missed");
    };
    assert_eq!(notice.tone, CenterTone::Danger);
    assert_eq!(notice.provider_ids, [1]);
    assert_eq!(notice.run_number, None);

    assert_eq!(
        serde_json::from_value::<NotificationCenterChange>(json!({ "type": "read" })).unwrap(),
        NotificationCenterChange::Read { key: None }
    );
    assert_eq!(
        serde_json::from_value::<NotificationCenterChange>(json!({ "type": "clear" })).unwrap(),
        NotificationCenterChange::Clear
    );
    assert!(
        serde_json::from_value::<NotificationCenterChange>(json!({ "type": "bogus" })).is_err()
    );

    let value = serde_json::to_value(push(Default::default(), notice.clone())).unwrap();
    assert_eq!(value["revision"], 1);
    assert_eq!(value["unread"], 1);
    assert_eq!(value["items"][0]["providerIds"], json!([1]));
    assert_eq!(value["items"][0]["pipelineId"], "p");
    assert_eq!(value["items"][0]["runNumber"], json!(null));
    assert_eq!(value["items"][0]["tone"], "danger");
}

#[test]
fn a_shown_notification_keeps_its_metadata_and_takes_its_tone_from_the_kind() {
    for (kind, tone) in [
        (AlertKind::Failed, CenterTone::Danger),
        (AlertKind::Outage, CenterTone::Warning),
        (AlertKind::Recovered, CenterTone::Success),
    ] {
        let source = shown(kind);
        let notice = CenterNotification::from(&source);

        assert_eq!(notice.tone, tone);
        assert_eq!(notice.key, "provider:1");
        assert_eq!(notice.at, 1_700_000_000_000);
        assert!(!notice.read);
        assert_eq!(notice.level, Some(HealthLevel::MajorOutage));
        assert_eq!(notice.provider_ids, vec![1, 2]);
        assert_eq!(notice.link, source.notice.link);
        assert_eq!(notice.run_number, None);
    }
}

#[test]
fn a_notification_frame_from_a_server_maps_like_the_local_one() {
    let source = shown(AlertKind::Degraded);
    let payload = serde_json::to_value(&source).unwrap();

    let received = CenterNotification::received(&payload).unwrap();
    let local = CenterNotification::from(&source);

    assert_eq!(received.key, local.key);
    assert_eq!(received.at, local.at);
    assert_eq!(received.tone, CenterTone::Danger);
    assert_eq!(received.title, local.title);
    assert_eq!(received.provider_ids, local.provider_ids);
    assert_eq!(received.link, local.link);
    assert!(CenterNotification::received(&json!({ "key": "x" })).is_none());
}

#[test]
fn a_watched_run_names_its_outcome_and_keeps_the_run_number() {
    for (outcome, tone, result) in [
        (WatchOutcome::Succeeded, CenterTone::Success, "succeeded"),
        (WatchOutcome::Failed, CenterTone::Danger, "failed"),
        (
            WatchOutcome::Cancelled,
            CenterTone::Warning,
            "was cancelled",
        ),
        (WatchOutcome::Skipped, CenterTone::Warning, "was skipped"),
        (WatchOutcome::Finished, CenterTone::Warning, "finished"),
    ] {
        let notice = CenterNotification::watched("s1", 42, "stub__1__api", "api", "Stub", outcome);

        assert_eq!(notice.key, "watch:s1");
        assert_eq!(notice.tone, tone);
        assert_eq!(notice.title, format!("Watched run #42 {result}"));
        assert_eq!(notice.body, "api · Stub");
        assert_eq!(notice.pipeline_id.as_deref(), Some("stub__1__api"));
        assert_eq!(notice.run_number, Some(42));
        assert!(!notice.read);
        assert!(notice.provider_ids.is_empty());
    }
}
