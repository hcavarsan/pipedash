use pipedash_core::{
    domain::{
        notification_center::{
            CenterNotification,
            CenterTone,
        },
        DomainError,
        PipelineMenuSnapshot,
    },
    event::CoreEvent,
};

use super::{
    projection::{
        parse_menu,
        waking,
    },
    state::{
        Inner,
        Origin,
        Ticket,
    },
    tray::{
        ClientLink,
        TrayStatus,
    },
    *,
};

fn notice(key: &str) -> CenterNotification {
    CenterNotification {
        key: key.to_string(),
        at: 1,
        read: false,
        tone: CenterTone::Danger,
        level: None,
        title: key.to_string(),
        body: String::new(),
        provider_ids: Vec::new(),
        pipeline_id: None,
        link: None,
        run_number: None,
    }
}

#[test]
fn a_window_larger_than_the_work_area_is_fitted_to_it() {
    let work_area = tauri::PhysicalSize::new(1470, 924);
    let window = tauri::PhysicalSize::new(1700, 1000);

    assert!(window_exceeds_work_area(window, work_area));
    assert!(window_exceeds_work_area(
        window,
        tauri::PhysicalSize::new(1920, 985)
    ));
    assert!(!window_exceeds_work_area(
        tauri::PhysicalSize::new(1280, 800),
        work_area
    ));
    assert!(!window_exceeds_work_area(work_area, work_area));
}

#[test]
fn menu_read_failure_reports_the_error_and_recovery_clears_it() {
    let mut inner = Inner::default();
    inner.apply_menu_result(Err(DomainError::DatabaseError(
        "Pinned view unavailable".to_string(),
    )));
    assert!(inner.snapshot.is_none());
    assert!(inner.data_error.is_some());

    inner.apply_menu_result(Ok(PipelineMenuSnapshot {
        pipelines: Vec::new(),
        shortcuts: Vec::new(),
    }));
    assert!(inner.snapshot.is_some());
    assert!(inner.data_error.is_none());
}

#[test]
fn server_notifications_need_no_local_core_and_core_ones_need_an_attached_core() {
    let inner = Inner::default();

    assert!(inner.accepts(Origin::Server));
    assert!(!inner.accepts(Origin::Core(0)));
}

#[test]
fn recorded_notifications_reach_the_history_and_follow_the_system_switch() {
    let mut inner = Inner::default();

    let (history, deliver) = inner.record(&notice("first"));
    assert!(!deliver);
    assert_eq!(history.items.len(), 1);
    assert_eq!(history.items[0].key, "first");

    inner.saved.settings.notifications_enabled = true;
    let (history, deliver) = inner.record(&notice("second"));
    assert!(deliver);
    assert_eq!(history.items.len(), 2);
    assert_eq!(history.items[0].key, "second");
    assert_eq!(inner.history.items.len(), 2);
}

#[test]
fn the_status_json_keeps_its_fields_and_adds_the_tray_status_and_unread_count() {
    let mut inner = Inner {
        generation: 1,
        vault_ready: true,
        ..Default::default()
    };
    let quiet = serde_json::to_value(inner.status(&Default::default())).unwrap();
    inner.history.push(notice("broken"));
    let failing = serde_json::to_value(inner.status(&Default::default())).unwrap();

    assert_eq!(quiet["tray_status"], "idle");
    assert_eq!(quiet["unread"], 0);
    assert_eq!(failing["tray_status"], "failing");
    assert_eq!(failing["unread"], 1);
    for field in [
        "revision",
        "settings",
        "tray_state",
        "ready",
        "notification_preference_imported",
        "error",
    ] {
        assert!(failing.get(field).is_some(), "{field}");
    }
    assert_eq!(failing["tray_state"], "off");
    assert_eq!(failing["ready"], true);
}

#[test]
fn the_status_serializes_every_tray_state_in_lower_case() {
    for (status, name) in [
        (TrayStatus::Offline, "offline"),
        (TrayStatus::Failing, "failing"),
        (TrayStatus::Running, "running"),
        (TrayStatus::Attention, "attention"),
        (TrayStatus::Idle, "idle"),
    ] {
        assert_eq!(serde_json::to_value(status).unwrap(), name);
    }
}

#[test]
fn a_client_is_active_only_while_connected_whatever_the_local_core_says() {
    use crate::remote::model::RemoteStatus;

    let link = |status| {
        Some(ClientLink {
            host: "ci.example.com".to_string(),
            status,
        })
    };
    let mut inner = Inner {
        vault_ready: true,
        client: link(RemoteStatus::Reconnecting),
        ..Default::default()
    };
    assert!(!inner.active());

    inner.vault_ready = false;
    inner.client = link(RemoteStatus::Connected);
    assert!(inner.active());
    assert!(inner.current(inner.ticket()));
    assert!(!inner.current(Ticket {
        generation: 9,
        epoch: 9
    }));

    inner.client = None;
    assert!(!inner.active());
}

#[test]
fn server_pins_parse_and_failures_become_a_menu_error() {
    let snapshot = PipelineMenuSnapshot {
        pipelines: Vec::new(),
        shortcuts: Vec::new(),
    };
    let body = serde_json::to_string(&snapshot).unwrap();
    let mut inner = Inner::default();

    inner.apply_menu_result(parse_menu(200, &body));
    assert!(inner.snapshot.is_some());
    assert!(inner.data_error.is_none());

    for (status, body) in [(500, body.as_str()), (200, "not json"), (200, "{}")] {
        inner.apply_menu_result(parse_menu(status, body));
        assert!(inner.snapshot.is_none());
        assert!(inner.data_error.is_some());
    }
}

#[test]
fn only_pipeline_and_provider_changes_wake_the_tray() {
    for name in [
        "pipeline-views-changed",
        "pipelines-updated",
        "pipeline-status-changed",
        "providers-changed",
        "provider-added",
        "provider-updated",
        "provider-removed",
        "provider-status-updated",
    ] {
        assert!(waking(name), "{name}");
    }
    assert!(waking(CoreEvent::PipelineViewsChanged.event_name()));
    assert!(waking(CoreEvent::ProvidersChanged.event_name()));
    for name in [
        "run-triggered",
        "notification",
        "connection-status",
        "vault-locked",
    ] {
        assert!(!waking(name), "{name}");
    }
}

#[test]
fn notices_and_old_menu_actions_end_when_the_saved_server_or_mode_changes() {
    use crate::remote::model::RemoteStatus;

    let link = |host: &str| {
        Some(ClientLink {
            host: host.to_string(),
            status: RemoteStatus::Connected,
        })
    };
    let a = || Some("d1@https://a.example.com".to_string());
    let mut inner = Inner::default();
    inner.history.push(notice("from-a"));
    let ticket = inner.ticket();
    assert!(!inner.follow(link("a.example.com"), a()));
    assert_eq!(inner.history.items.len(), 1);
    assert_eq!(inner.ticket(), ticket);
    let revision = inner.history.revision;

    assert!(!inner.follow(link("a.example.com"), a()));
    assert_eq!(inner.history.items.len(), 1);
    assert_eq!(inner.ticket(), ticket);

    assert!(inner.follow(
        link("b.example.com"),
        Some("d2@https://b.example.com".to_string())
    ));
    assert!(inner.history.items.is_empty());
    assert!(inner.history.revision > revision);
    assert_ne!(inner.ticket(), ticket);
    assert!(!inner.current(ticket));

    inner.history.push(notice("from-b"));
    let ticket = inner.ticket();
    assert!(inner.follow(None, None));
    assert!(inner.history.items.is_empty());
    assert_ne!(inner.ticket(), ticket);
}

#[test]
fn the_first_observation_of_standalone_mode_keeps_notices_and_a_later_client_switch_clears_them() {
    let mut inner = Inner::default();
    inner.history.push(notice("early"));
    assert!(!inner.follow(None, None));
    assert_eq!(inner.history.items.len(), 1);

    assert!(inner.follow(None, Some("d1@https://a.example.com".to_string())));
    assert!(inner.history.items.is_empty());
}

#[test]
fn a_notice_from_the_new_server_survives_the_menu_refresh_that_sees_the_switch() {
    let b = || Some("d2@https://b.example.com".to_string());
    let mut inner = Inner::default();
    assert!(!inner.follow(None, Some("d1@https://a.example.com".to_string())));
    inner.history.push(notice("from-a"));

    assert!(inner.adopt_server(b()));
    inner.record(&notice("from-b"));
    assert!(!inner.follow(None, b()));

    assert_eq!(
        inner
            .history
            .items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>(),
        ["from-b"]
    );
}

#[test]
fn a_server_switch_drops_the_old_pins_and_a_menu_reply_fetched_before_it() {
    use crate::remote::model::RemoteStatus;

    let pins = || PipelineMenuSnapshot {
        pipelines: Vec::new(),
        shortcuts: Vec::new(),
    };
    let mut inner = Inner::default();
    inner.follow(
        Some(ClientLink {
            host: "a.example.com".to_string(),
            status: RemoteStatus::Connected,
        }),
        Some("d1@https://a.example.com".to_string()),
    );
    inner.apply_menu_result(Ok(pins()));
    let fetched_for_a = inner.ticket();

    assert!(inner.adopt_server(Some("d2@https://b.example.com".to_string())));
    assert!(inner.snapshot.is_none());
    assert!(!inner.current(fetched_for_a));
}
