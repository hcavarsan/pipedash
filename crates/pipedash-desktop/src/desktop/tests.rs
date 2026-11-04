use pipedash_core::domain::DomainError;

use super::*;

fn notice(key: &str) -> DesktopNotice {
    DesktopNotice {
        key: key.to_string(),
        at: 1,
        read: false,
        tone: "danger",
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
