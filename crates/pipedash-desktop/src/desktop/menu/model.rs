use pipedash_core::domain::{
    notification_center::{
        CenterNotification,
        CenterTone,
    },
    NotificationPreferences,
    PinKind,
    PipelineStatus,
};
use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchRunStatus,
};

use super::actions::Command;
use crate::{
    desktop::{
        state::Inner,
        tray,
    },
    native::NativeUiAction,
};

pub(super) struct Row {
    pub(super) key: String,
    pub(super) label: String,
    pub(super) enabled: bool,
    pub(super) checked: Option<bool>,
    pub(super) command: Option<Command>,
    pub(super) children: Option<Vec<Row>>,
    pub(super) separator: bool,
}

impl Row {
    fn item(key: &str, label: &str, command: Option<Command>) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            enabled: command.is_some(),
            checked: None,
            command,
            children: None,
            separator: false,
        }
    }
    fn section(key: &str) -> Self {
        Self {
            separator: true,
            ..Self::item(key, "", None)
        }
    }
    fn submenu(key: &str, label: &str, children: Vec<Row>) -> Self {
        Self {
            children: Some(children),
            enabled: true,
            ..Self::item(key, label, None)
        }
    }
    fn link(key: &str, label: &str, path: &str) -> Self {
        Self::item(key, label, Some(Command::Navigate(path.to_string(), None)))
    }
}

pub(super) fn label(value: &str) -> String {
    let mut chars = value.chars();
    let mut output: String = chars
        .by_ref()
        .take(48)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if chars.next().is_some() {
        output.push('…');
    }
    output.replace('&', "&&")
}

fn context(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .replace('&', "&&")
}

fn status(value: &PipelineStatus) -> &'static str {
    match value {
        PipelineStatus::Pending => "Pending",
        PipelineStatus::Running => "Running",
        PipelineStatus::Success => "Succeeded",
        PipelineStatus::Failed => "Failed",
        PipelineStatus::Degraded => "Degraded",
        PipelineStatus::Cancelled => "Cancelled",
        PipelineStatus::Skipped => "Skipped",
    }
}

fn watch_status(value: BuildWatchRunStatus) -> &'static str {
    match value {
        BuildWatchRunStatus::Pending => "Pending",
        BuildWatchRunStatus::Running => "Running",
        BuildWatchRunStatus::Success => "Succeeded",
        BuildWatchRunStatus::Failed => "Failed",
        BuildWatchRunStatus::Cancelled => "Cancelled",
        BuildWatchRunStatus::Skipped => "Skipped",
    }
}

fn notice_prefix(tone: CenterTone) -> &'static str {
    match tone {
        CenterTone::Danger => "Failed",
        CenterTone::Warning => "Warning",
        CenterTone::Success => "Finished",
    }
}

fn notice_target(notice: &CenterNotification) -> (String, Option<u64>) {
    match &notice.pipeline_id {
        Some(id) => (
            format!("/pipelines/{}", encode(id)),
            notice
                .run_number
                .and_then(|number| u64::try_from(number).ok()),
        ),
        None => ("/notifications".to_string(), None),
    }
}

pub(super) fn encode(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

pub(super) fn model(inner: &Inner, prefs: &NotificationPreferences) -> Vec<Row> {
    let mut rows = vec![Row::item("open", "Open Pipedash", Some(Command::Open))];
    if !inner.active() {
        let line = inner
            .data_error
            .clone()
            .unwrap_or_else(|| tray::offline_line(inner));
        rows.push(Row::item("readiness", &context(&line), None));
        rows.push(Row::section("end"));
        rows.push(Row::item("quit", "Quit Pipedash", Some(Command::Quit)));
        return rows;
    }
    rows.push(Row::item(
        "status",
        &context(&tray::summary(inner, prefs)),
        None,
    ));
    if let Some(error) = inner.data_error.as_ref().or(inner.refresh_message.as_ref()) {
        rows.push(Row::item("error", error, None));
        rows.push(Row::link(
            "provider-error",
            "Open provider settings…",
            "/settings/providers",
        ));
    }
    rows.push(Row::section("notices-separator"));
    let unread: Vec<&CenterNotification> = inner
        .history
        .items
        .iter()
        .filter(|notice| !notice.read)
        .collect();
    for notice in unread.iter().take(3) {
        let (path, run) = notice_target(notice);
        rows.push(Row::item(
            &format!("notice:{}", notice.key),
            &format!("{} · {}", notice_prefix(notice.tone), label(&notice.title)),
            Some(Command::Notice(notice.key.clone(), path, run)),
        ));
    }
    if !unread.is_empty() {
        rows.push(Row::item(
            "read-all",
            "Mark all as read",
            Some(Command::MarkAllRead),
        ));
    }
    rows.push(Row::link(
        "notifications",
        &if unread.is_empty() {
            "Notifications…".to_string()
        } else {
            format!("Notifications ({} unread)…", unread.len())
        },
        "/notifications",
    ));
    rows.push(Row::section("watch-separator"));
    if let Some(watch) = &inner.watch {
        let monitoring = match watch.monitoring {
            BuildWatchMonitoring::Watching => "Watching",
            BuildWatchMonitoring::Completed => "Watch completed",
            BuildWatchMonitoring::Stopped => "Watch stopped",
            BuildWatchMonitoring::Interrupted => "Watch interrupted",
        };
        rows.push(Row::item("watch-header", monitoring, None));
        rows.push(Row::item(
            "watch-info",
            &format!(
                "{} #{} · {}",
                label(&watch.pipeline_name),
                watch.run_number,
                watch_status(watch.status)
            ),
            None,
        ));
        rows.push(Row::item(
            &format!("watch-open:{}", watch.session_id),
            "Open run",
            Some(Command::Navigate(
                format!("/pipelines/{}", encode(&watch.pipeline_id)),
                u64::try_from(watch.run_number).ok(),
            )),
        ));
        if watch.monitoring == BuildWatchMonitoring::Watching {
            rows.push(Row::item(
                &format!("watch-stop:{}", watch.session_id),
                "Stop watching",
                Some(Command::StopWatch(watch.session_id.clone())),
            ));
        }
        rows.push(Row::section("pins-separator"));
    }
    rows.push(Row::item("pinned-header", "Pinned pipelines", None));
    if let Some(snapshot) = &inner.snapshot {
        if snapshot.pipelines.is_empty() {
            rows.push(Row::item("empty-pins", "No pinned pipelines", None));
        }
        for entry in snapshot.pipelines.iter().take(8) {
            let key = format!(
                "pipeline:{}:{}:{}",
                inner.generation, inner.epoch, entry.pipeline_id
            );
            let pipeline = entry.pipeline.as_ref();
            let name = pipeline
                .map(|pipeline| pipeline.name.as_str())
                .unwrap_or(&entry.pipeline_id);
            let title = format!(
                "{} · {}",
                label(name),
                pipeline
                    .map(|pipeline| status(&pipeline.status))
                    .unwrap_or("Unavailable")
            );
            let path = format!("/pipelines/{}", encode(&entry.pipeline_id));
            let mut children = Vec::new();
            if let Some(pipeline) = pipeline {
                children.push(Row::item(
                    "context",
                    &format!(
                        "{} / {}",
                        context(entry.provider_name.as_deref().unwrap_or("Provider")),
                        context(&pipeline.repository)
                    ),
                    None,
                ));
                children.push(Row::link("pipeline-open", "Open pipeline", &path));
                children.push(Row::link("pipeline-watch", "Choose run to watch…", &path));
            }
            children.push(Row::item(
                "pipeline-unpin",
                "Unpin",
                Some(Command::Unpin(entry.pipeline_id.clone())),
            ));
            rows.push(Row::submenu(&key, &title, children));
        }
        if snapshot.pipelines.len() > 8 {
            rows.push(Row::link(
                "overflow",
                "Show all pinned pipelines…",
                "/pipelines?view=pinned",
            ));
        }
        rows.push(Row::item(
            "manage-pins",
            "Manage pins…",
            Some(Command::Ui(NativeUiAction::ManagePins)),
        ));
        if !snapshot.shortcuts.is_empty() {
            let mut children: Vec<Row> = snapshot
                .shortcuts
                .iter()
                .map(|pin| {
                    let path = match pin.kind {
                        PinKind::View => format!("/pipelines?view={}", encode(&pin.reference)),
                        PinKind::Repository => {
                            format!("/pipelines?search={}", encode(&pin.reference))
                        }
                    };
                    Row::link(
                        &format!(
                            "shortcut:{}:{}:{}:{}",
                            inner.generation,
                            inner.epoch,
                            pin.kind.as_str(),
                            pin.reference
                        ),
                        &label(&pin.name),
                        &path,
                    )
                })
                .collect();
            children.push(Row::item(
                "manage-views",
                "Manage views…",
                Some(Command::Ui(NativeUiAction::ManageViews)),
            ));
            rows.push(Row::submenu(
                "shortcuts",
                "Pinned views and repositories",
                children,
            ));
        }
    } else {
        rows.push(Row::item(
            "pins-loading",
            if inner.data_error.is_some() {
                "Couldn't load pinned pipelines"
            } else {
                "Loading pinned pipelines…"
            },
            None,
        ));
        rows.push(Row::item(
            "manage-pins",
            "Manage pins…",
            Some(Command::Ui(NativeUiAction::ManagePins)),
        ));
    }
    rows.push(Row::section("navigation-separator"));
    rows.push(Row::link("all", "All pipelines", "/pipelines"));
    let mut refresh = Row::item("refresh", "Refresh all", Some(Command::Refresh));
    refresh.enabled = !inner.refresh_active;
    rows.push(refresh);
    let mut notifications = Row::item(
        "system-notifications",
        "System notifications",
        Some(Command::Notifications),
    );
    notifications.checked = Some(inner.saved.settings.notifications_enabled);
    rows.push(Row::submenu(
        "settings",
        "Settings",
        vec![
            Row::link("general", "General…", "/settings/general"),
            Row::link(
                "notification-settings",
                "Notifications…",
                "/settings/notifications",
            ),
            Row::link("providers", "Providers…", "/settings/providers"),
            notifications,
        ],
    ));
    rows.push(Row::section("end"));
    if inner.client.is_none() {
        rows.push(Row::item("lock", "Lock Pipedash", Some(Command::Lock)));
    }
    rows.push(Row::item("quit", "Quit Pipedash", Some(Command::Quit)));
    rows
}
