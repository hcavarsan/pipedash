use pipedash_core::domain::{
    notification_center::{
        CenterNotification,
        CenterTone,
    },
    NotificationPreferences,
    PinKind,
    PipelineStatus,
    RefreshResult,
    RefreshState,
};

use super::{
    actions::{
        refresh_message,
        Command,
        REFRESH_FAILED,
    },
    model::{
        encode,
        label,
        model,
        Row,
    },
    tray,
};
use crate::{
    desktop::state::Inner,
    native::NativeUiAction,
};

#[test]
fn dynamic_labels_cannot_inject_rows_or_mnemonics() {
    assert_eq!(
        label("Build & deploy\nNext\trow"),
        "Build && deploy Next row"
    );
    assert_eq!(label(&"é".repeat(49)), format!("{}…", "é".repeat(48)));
}

#[test]
fn pipeline_routes_encode_separators_and_percent_once() {
    assert_eq!(encode("a/b%#é"), "a%2Fb%25%23%C3%A9");
}

fn pinned_state() -> Inner {
    Inner {
        saved: Default::default(),
        revision: 0,
        generation: 1,
        epoch: 1,
        vault_ready: true,
        core: None,
        snapshot: Some(pipedash_core::domain::PipelineMenuSnapshot {
            pipelines: (0..10)
                .map(|index| pipedash_core::domain::PipelineMenuEntry {
                    pipeline_id: format!("private-{index}"),
                    pipeline: None,
                    provider_name: None,
                })
                .collect(),
            shortcuts: vec![pipedash_core::domain::PinnedItem {
                kind: PinKind::Repository,
                reference: "acme/a&b".to_string(),
                name: "A & B".to_string(),
            }],
        }),
        settings_error: None,
        data_error: None,
        tray_error: None,
        tray_ready: false,
        refresh_active: false,
        refresh_message: None,
        watch_session: None,
        watch: None,
        history: Default::default(),
        client: None,
        server: None,
        retry: Default::default(),
        tasks: Default::default(),
    }
}

#[test]
fn missing_pins_keep_order_and_unpin_while_overflow_and_shortcuts_remain_reachable() {
    let rows = model(&pinned_state(), &Default::default());
    let pinned: Vec<_> = rows
        .iter()
        .filter(|row| row.key.starts_with("pipeline:"))
        .collect();
    assert_eq!(pinned.len(), 8);
    assert_eq!(pinned[0].label, "private-0 · Unavailable");
    assert_eq!(pinned[7].label, "private-7 · Unavailable");
    let unavailable = pinned[0].children.as_ref().unwrap();
    assert_eq!(unavailable.len(), 1);
    assert!(matches!(&unavailable[0].command, Some(Command::Unpin(id)) if id == "private-0"));
    assert!(rows.iter().any(|row| matches!(&row.command, Some(Command::Navigate(path, _)) if path == "/pipelines?view=pinned")));
    let shortcuts = rows
        .iter()
        .find(|row| row.key == "shortcuts")
        .unwrap()
        .children
        .as_ref()
        .unwrap();
    assert_eq!(shortcuts[0].label, "A && B");
    assert!(
        matches!(&shortcuts[0].command, Some(Command::Navigate(path, _)) if path == "/pipelines?search=acme%2Fa%26b")
    );
    assert!(matches!(
        &shortcuts[1].command,
        Some(Command::Ui(NativeUiAction::ManageViews))
    ));
}

#[test]
fn locking_hides_all_private_rows_and_service_actions() {
    let mut inner = pinned_state();
    inner.vault_ready = false;
    let rows = model(&inner, &Default::default());
    assert_eq!(
        rows.iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>(),
        ["Open Pipedash", "Pipedash is locked", "", "Quit Pipedash"]
    );
    assert!(rows.iter().all(|row| row.children.is_none()));
}

#[test]
fn attachment_replacement_retires_old_pipeline_action_ids() {
    let mut inner = pinned_state();
    let first: Vec<_> = model(&inner, &Default::default())
        .into_iter()
        .filter(|row| row.key.starts_with("pipeline:"))
        .map(|row| row.key)
        .collect();
    inner.epoch += 1;
    let second: Vec<_> = model(&inner, &Default::default())
        .into_iter()
        .filter(|row| row.key.starts_with("pipeline:"))
        .map(|row| row.key)
        .collect();
    assert!(first.iter().all(|key| !second.contains(key)));
}

fn notice(
    key: &str, tone: CenterTone, pipeline: Option<(&str, Option<i64>)>,
) -> CenterNotification {
    CenterNotification {
        key: key.to_string(),
        at: 1,
        read: false,
        tone,
        level: None,
        title: format!("Title {key}"),
        body: String::new(),
        provider_ids: Vec::new(),
        pipeline_id: pipeline.map(|(id, _)| id.to_string()),
        link: None,
        run_number: pipeline.and_then(|(_, run)| run),
    }
}

fn labels(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.label.as_str()).collect()
}

#[test]
fn the_three_newest_unread_notices_open_their_target_and_the_count_shows_in_the_menu() {
    let mut inner = pinned_state();
    inner.history.push(notice("old", CenterTone::Success, None));
    inner.history.mark_read(None);
    inner.history.push(notice(
        "a",
        CenterTone::Danger,
        Some(("acme/api", Some(42))),
    ));
    inner
        .history
        .push(notice("b", CenterTone::Warning, Some(("acme/web", None))));
    inner.history.push(notice("c", CenterTone::Success, None));
    inner.history.push(notice("d", CenterTone::Danger, None));

    let rows = model(&inner, &Default::default());
    let notices: Vec<_> = rows
        .iter()
        .filter(|row| row.key.starts_with("notice:"))
        .collect();

    assert_eq!(
        notices
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Failed · Title d",
            "Finished · Title c",
            "Warning · Title b"
        ]
    );
    assert!(matches!(
        &notices[2].command,
        Some(Command::Notice(key, path, None)) if key == "b" && path == "/pipelines/acme%2Fweb"
    ));
    assert!(matches!(
        &notices[0].command,
        Some(Command::Notice(key, path, None)) if key == "d" && path == "/notifications"
    ));
    assert!(rows
        .iter()
        .any(|row| row.key == "read-all" && matches!(row.command, Some(Command::MarkAllRead))));
    let center = rows.iter().find(|row| row.key == "notifications").unwrap();
    assert_eq!(center.label, "Notifications (4 unread)…");
    assert!(
        matches!(&center.command, Some(Command::Navigate(path, None)) if path == "/notifications")
    );
    assert_eq!(rows[1].label, "4 unread");
    assert!(rows.iter().any(|row| row.key == "settings"));
}

#[test]
fn an_exact_run_notice_targets_the_run() {
    let mut inner = pinned_state();
    inner.history.push(notice(
        "run",
        CenterTone::Danger,
        Some(("acme/api", Some(42))),
    ));

    let rows = model(&inner, &Default::default());
    let row = rows.iter().find(|row| row.key == "notice:run").unwrap();

    assert!(matches!(
        &row.command,
        Some(Command::Notice(key, path, Some(42))) if key == "run" && path == "/pipelines/acme%2Fapi"
    ));
}

#[test]
fn without_unread_notices_only_the_plain_notifications_row_remains() {
    let mut inner = pinned_state();
    inner.history.push(notice("seen", CenterTone::Danger, None));
    inner.history.mark_read(None);

    let rows = model(&inner, &Default::default());

    assert!(rows.iter().all(|row| !row.key.starts_with("notice:")));
    assert!(rows.iter().all(|row| row.key != "read-all"));
    assert_eq!(
        rows.iter()
            .find(|row| row.key == "notifications")
            .unwrap()
            .label,
        "Notifications…"
    );
    assert_eq!(rows[1].label, "All quiet");
    assert!(rows.iter().any(|row| row.key == "lock"));
}

#[test]
fn the_status_row_follows_the_device_preferences() {
    let mut inner = pinned_state();
    inner.snapshot = Some(pipedash_core::domain::PipelineMenuSnapshot {
        pipelines: vec![pipedash_core::domain::PipelineMenuEntry {
            pipeline_id: "api".to_string(),
            pipeline: Some(pipedash_core::domain::Pipeline {
                id: "api".to_string(),
                provider_id: 1,
                provider_type: "stub".to_string(),
                name: "api".to_string(),
                status: PipelineStatus::Failed,
                last_run: None,
                last_updated: chrono::Utc::now(),
                repository: "acme/api".to_string(),
                branch: None,
                workflow_file: None,
                resource_state: None,
                metadata: Default::default(),
                overdue_run: None,
                typical_duration_seconds: None,
                actions: Default::default(),
            }),
            provider_name: None,
        }],
        shortcuts: Vec::new(),
    });
    let muted = NotificationPreferences {
        muted_pipelines: vec!["api".to_string()],
        ..Default::default()
    };

    assert_eq!(model(&inner, &Default::default())[1].label, "1 failing");
    assert_eq!(model(&inner, &muted)[1].label, "All quiet");
}

#[test]
fn client_mode_shows_the_server_pins_and_hides_what_only_works_locally() {
    let mut inner = pinned_state();
    inner.vault_ready = false;
    inner.client = Some(tray::ClientLink {
        host: "ci.example.com".to_string(),
        status: crate::remote::model::RemoteStatus::Connected,
    });

    let rows = model(&inner, &Default::default());

    assert!(
        rows.iter()
            .filter(|row| row.key.starts_with("pipeline:"))
            .count()
            == 8
    );
    assert!(rows
        .iter()
        .any(|row| row.key == "refresh" && matches!(row.command, Some(Command::Refresh))));
    assert!(rows.iter().all(|row| row.key != "lock"));
    assert!(rows
        .iter()
        .all(|row| !matches!(row.command, Some(Command::Lock))));
    assert!(rows.iter().any(|row| row.key == "quit"));
}

#[test]
fn a_client_that_is_not_connected_shows_the_host_and_nothing_private() {
    let mut inner = pinned_state();
    inner.vault_ready = false;
    inner.client = Some(tray::ClientLink {
        host: "ci.example.com".to_string(),
        status: crate::remote::model::RemoteStatus::Reconnecting,
    });

    let rows = model(&inner, &Default::default());

    assert_eq!(
        labels(&rows),
        [
            "Open Pipedash",
            "Reconnecting to ci.example.com",
            "",
            "Quit Pipedash"
        ]
    );
    assert!(rows.iter().all(|row| row.children.is_none()));

    inner.client = Some(tray::ClientLink {
        host: "ci.example.com".to_string(),
        status: crate::remote::model::RemoteStatus::Removed,
    });
    assert_eq!(
        model(&inner, &Default::default())[1].label,
        "Removed from ci.example.com"
    );
}

#[test]
fn refresh_messages_prefer_failures_over_pending_over_missing() {
    let result = |state| RefreshResult {
        provider_id: 1,
        state,
        kind: None,
        message: None,
    };

    assert_eq!(refresh_message(&[result(RefreshState::Ok)]), None);
    assert_eq!(
        refresh_message(&[
            result(RefreshState::NotFound),
            result(RefreshState::Pending)
        ]),
        Some("Refresh still in progress".to_string())
    );
    assert_eq!(
        refresh_message(&[result(RefreshState::Pending), result(RefreshState::Failed)]),
        Some(REFRESH_FAILED.to_string())
    );
}
