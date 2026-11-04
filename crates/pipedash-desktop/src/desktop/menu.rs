use std::{
    collections::HashMap,
    sync::{
        Mutex,
        PoisonError,
    },
    time::Duration,
};

use pipedash_core::domain::{
    NotificationPreferences,
    PinKind,
    PipelineStatus,
    RefreshResult,
    RefreshState,
    ViewRef,
};
use tauri::{
    image::Image,
    menu::{
        CheckMenuItem,
        Menu,
        MenuItem,
        MenuItemKind,
        PredefinedMenuItem,
        Submenu,
    },
    tray::TrayIconBuilder,
    AppHandle,
    Manager,
    Runtime,
};
use tauri_plugin_pipedash_native::{
    BuildWatchMonitoring,
    BuildWatchRunStatus,
};

use super::{
    notifications::DesktopNotice,
    tray::{
        self,
        TrayStatus,
    },
    DesktopSettingsPatch,
    DesktopState,
    Inner,
    Ticket,
};
use crate::{
    native::{
        self,
        NativeIntent,
        NativeUiAction,
    },
    remote::{
        alerts::{
            client_request,
            client_request_within,
        },
        RemoteConnection,
    },
};

const TRAY_ID: &str = "pipedash-main";

#[derive(Clone)]
pub(super) enum Command {
    Open,
    Navigate(String, Option<u64>),
    Ui(NativeUiAction),
    Unpin(String),
    StopWatch(String),
    Refresh,
    Notifications,
    Lock,
    Notice(String, String, Option<u64>),
    MarkAllRead,
    Quit,
}

struct Row {
    key: String,
    label: String,
    enabled: bool,
    checked: Option<bool>,
    command: Option<Command>,
    children: Option<Vec<Row>>,
    separator: bool,
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

struct Node {
    key: String,
    label: String,
    enabled: bool,
    checked: Option<bool>,
    item: MenuItemKind<tauri::Wry>,
    children: Vec<Node>,
}

struct MenuOwner {
    menu: Menu<tauri::Wry>,
    nodes: Vec<Node>,
    commands: HashMap<String, (Command, Ticket)>,
    next_id: u64,
    applied: Applied,
}

struct MenuState(Mutex<MenuOwner>);

pub(super) fn initialize(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::new(app)?;
    app.manage(MenuState(Mutex::new(MenuOwner {
        menu,
        nodes: Vec::new(),
        commands: HashMap::new(),
        next_id: 0,
        applied: Applied::default(),
    })));
    apply(app)
}

pub(super) fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    let menu = app
        .state::<MenuState>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .menu
        .clone();
    let icon = base_icon(app)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let command = app
                .state::<MenuState>()
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .commands
                .get(event.id().as_ref())
                .cloned();
            if let Some((command, ticket)) = command {
                dispatch(app, command, Some(ticket));
            }
        })
        .build(app)?;
    reset_applied(app);
    Ok(())
}

pub(super) fn remove_tray(app: &AppHandle) -> tauri::Result<()> {
    drop(app.remove_tray_by_id(TRAY_ID));
    reset_applied(app);
    Ok(())
}

fn reset_applied(app: &AppHandle) {
    app.state::<MenuState>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .applied = Applied::default();
}

pub(super) fn schedule_render<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<DesktopState>() {
        (state.render)();
    }
}

pub(super) fn render(app: &AppHandle) {
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if let Err(error) = apply(&handle) {
            tracing::warn!(%error, "Couldn't reconcile the native menu");
            handle.state::<DesktopState>().lock().tray_error =
                Some("Couldn't update the native menu. Retry in General settings.".to_string());
            super::publish_status(&handle);
        }
    }) {
        tracing::warn!(%error, "Couldn't schedule the native menu");
    }
}

pub(super) fn apply(app: &AppHandle) -> tauri::Result<()> {
    let prefs = super::preferences(app);
    let (rows, ticket, look) = {
        let state = app.state::<DesktopState>();
        let inner = state.lock();
        let look = Look {
            status: tray::tray_status(&inner, &prefs),
            title: tray::title(&inner),
            tooltip: tray::tooltip(&inner, &prefs),
        };
        (model(&inner, &prefs), inner.ticket(), look)
    };
    let state = app.state::<MenuState>();
    let mut owner = state.0.lock().unwrap_or_else(PoisonError::into_inner);
    let MenuOwner {
        menu,
        nodes,
        commands,
        next_id,
        applied,
    } = &mut *owner;
    commands.clear();
    reconcile(
        app,
        &Container::Menu(menu.clone()),
        nodes,
        rows,
        commands,
        next_id,
        ticket,
    )?;
    paint(app, &look, applied);
    Ok(())
}

enum Container {
    Menu(Menu<tauri::Wry>),
    Submenu(Submenu<tauri::Wry>),
}

impl Container {
    fn insert(&self, item: &MenuItemKind<tauri::Wry>, index: usize) -> tauri::Result<()> {
        match self {
            Self::Menu(menu) => menu.insert(item, index),
            Self::Submenu(menu) => menu.insert(item, index),
        }
    }
    fn remove(&self, item: &MenuItemKind<tauri::Wry>) -> tauri::Result<()> {
        match self {
            Self::Menu(menu) => menu.remove(item),
            Self::Submenu(menu) => menu.remove(item),
        }
    }
}

fn reconcile(
    app: &AppHandle, parent: &Container, nodes: &mut Vec<Node>, rows: Vec<Row>,
    commands: &mut HashMap<String, (Command, Ticket)>, next_id: &mut u64, ticket: Ticket,
) -> tauri::Result<()> {
    let keys: Vec<String> = rows.iter().map(|row| row.key.clone()).collect();
    trim_nodes(parent, nodes, &keys)?;
    for (index, row) in rows.into_iter().enumerate() {
        let found = nodes.iter().position(|node| node.key == row.key);
        #[cfg(target_os = "linux")]
        let found = if let Some(found) = found.filter(|found| nodes[*found].label != row.label) {
            let node = nodes.remove(found);
            parent.remove(&node.item)?;
            None
        } else {
            found
        };
        if let Some(found) = found {
            if found != index {
                let node = nodes.remove(found);
                parent.remove(&node.item)?;
                parent.insert(&node.item, index)?;
                nodes.insert(index, node);
            }
        } else {
            *next_id += 1;
            let id = format!("desktop-menu-{next_id}");
            let item = if row.separator {
                MenuItemKind::Predefined(PredefinedMenuItem::separator(app)?)
            } else if row.children.is_some() {
                MenuItemKind::Submenu(Submenu::with_id(app, id, &row.label, row.enabled)?)
            } else if let Some(checked) = row.checked {
                MenuItemKind::Check(CheckMenuItem::with_id(
                    app,
                    id,
                    &row.label,
                    row.enabled,
                    checked,
                    None::<&str>,
                )?)
            } else {
                MenuItemKind::MenuItem(MenuItem::with_id(
                    app,
                    id,
                    &row.label,
                    row.enabled,
                    None::<&str>,
                )?)
            };
            parent.insert(&item, index)?;
            nodes.insert(
                index,
                Node {
                    key: row.key.clone(),
                    label: row.label.clone(),
                    enabled: row.enabled,
                    checked: row.checked,
                    item,
                    children: Vec::new(),
                },
            );
        }
        let node = &mut nodes[index];
        let text_changed = node.label != row.label;
        let enabled_changed = node.enabled != row.enabled;
        match &node.item {
            MenuItemKind::MenuItem(item) => {
                if text_changed {
                    item.set_text(&row.label)?;
                }
                if enabled_changed {
                    item.set_enabled(row.enabled)?;
                }
            }
            MenuItemKind::Submenu(item) => {
                if text_changed {
                    item.set_text(&row.label)?;
                }
                if enabled_changed {
                    item.set_enabled(row.enabled)?;
                }
                reconcile(
                    app,
                    &Container::Submenu(item.clone()),
                    &mut node.children,
                    row.children.unwrap_or_default(),
                    commands,
                    next_id,
                    ticket,
                )?;
            }
            MenuItemKind::Check(item) => {
                if text_changed {
                    item.set_text(&row.label)?;
                }
                if enabled_changed {
                    item.set_enabled(row.enabled)?;
                }
                if node.checked != row.checked {
                    item.set_checked(row.checked.unwrap_or(false))?;
                }
            }
            _ => {}
        }
        node.label = row.label;
        node.enabled = row.enabled;
        node.checked = row.checked;
        if let Some(command) = row.command {
            commands.insert(node.item.id().as_ref().to_string(), (command, ticket));
        }
    }
    Ok(())
}

fn trim_nodes(parent: &Container, nodes: &mut Vec<Node>, keys: &[String]) -> tauri::Result<()> {
    let mut index = 0;
    while index < nodes.len() {
        if keys.contains(&nodes[index].key) {
            index += 1;
        } else {
            let node = nodes.remove(index);
            parent.remove(&node.item)?;
        }
    }
    Ok(())
}

fn label(value: &str) -> String {
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

fn notice_prefix(tone: &str) -> &'static str {
    match tone {
        "danger" => "Failed",
        "warning" => "Warning",
        _ => "Finished",
    }
}

fn notice_target(notice: &DesktopNotice) -> (String, Option<u64>) {
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

struct Look {
    status: TrayStatus,
    title: Option<String>,
    tooltip: String,
}

#[derive(Default)]
struct Applied {
    icon: Option<TrayStatus>,
    title: Option<Option<String>>,
    tooltip: Option<String>,
}

fn base_icon(app: &AppHandle) -> tauri::Result<Image<'_>> {
    if cfg!(target_os = "macos") {
        Ok(tauri::include_image!("icons/tray-template.png"))
    } else {
        app.default_window_icon()
            .cloned()
            .ok_or(tauri::Error::WindowNotFound)
    }
}

fn paint(app: &AppHandle, look: &Look, applied: &mut Applied) {
    let Some(item) = app.tray_by_id(TRAY_ID) else {
        *applied = Applied::default();
        return;
    };
    let template = cfg!(target_os = "macos");
    if applied.icon != Some(look.status) {
        let painted = base_icon(app).and_then(|base| {
            let rgba = tray::paint(
                base.rgba(),
                base.width(),
                base.height(),
                look.status,
                template,
            );
            item.set_icon_with_as_template(
                Some(Image::new_owned(rgba, base.width(), base.height())),
                template,
            )
        });
        match painted {
            Ok(()) => applied.icon = Some(look.status),
            Err(error) => tracing::warn!(%error, "Couldn't paint the tray status"),
        }
    }
    if applied.title.as_ref() != Some(&look.title) {
        match item.set_title(look.title.as_deref()) {
            Ok(()) => applied.title = Some(look.title.clone()),
            Err(error) => tracing::warn!(%error, "Couldn't update the tray title"),
        }
    }
    if applied.tooltip.as_ref() != Some(&look.tooltip) {
        match item.set_tooltip(Some(&look.tooltip)) {
            Ok(()) => applied.tooltip = Some(look.tooltip.clone()),
            Err(error) => tracing::warn!(%error, "Couldn't update the tray tooltip"),
        }
    }
}

fn model(inner: &Inner, prefs: &NotificationPreferences) -> Vec<Row> {
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
    let unread: Vec<&DesktopNotice> = inner
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

pub(super) fn encode(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

pub(super) fn navigate(app: &AppHandle, path: &str, run_number: Option<u64>) {
    if let Err(error) = super::show_main_window(app) {
        tracing::warn!(%error, "Couldn't show the Pipedash window");
    }
    native::publish(
        app,
        NativeIntent::Link {
            path: path.to_string(),
            run_number,
        },
    );
}

pub(super) fn dispatch(app: &AppHandle, command: Command, ticket: Option<Ticket>) {
    {
        let state = app.state::<DesktopState>();
        let inner = state.lock();
        if ticket.is_some_and(|ticket| ticket != inner.ticket()) {
            return;
        }
        if !inner.active()
            && matches!(
                command,
                Command::Unpin(_)
                    | Command::StopWatch(_)
                    | Command::Refresh
                    | Command::Notifications
                    | Command::Lock
            )
        {
            return;
        }
    }
    match command {
        Command::Open => {
            if let Err(error) = super::show_main_window(app) {
                tracing::warn!(%error, "Couldn't show Pipedash");
            }
        }
        Command::Navigate(path, run) => navigate(app, &path, run),
        Command::Ui(action) => {
            if let Err(error) = super::show_main_window(app) {
                tracing::warn!(%error, "Couldn't show Pipedash");
            }
            native::publish(app, NativeIntent::Ui { action });
        }
        Command::Quit => app.exit(0),
        Command::Notice(key, path, run) => {
            super::notifications::mark_read(app, Some(&key));
            navigate(app, &path, run);
        }
        Command::MarkAllRead => {
            super::notifications::mark_read(app, None);
        }
        command => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let result = execute(&app, command, ticket).await;
                if result.is_err() {
                    let state = app.state::<DesktopState>();
                    let mut inner = state.lock();
                    if ticket.is_none_or(|ticket| inner.current(ticket)) {
                        inner.refresh_message = Some(
                            "Couldn't complete the native menu action. Open Pipedash for details."
                                .to_string(),
                        );
                    }
                    drop(inner);
                    super::wake(&app);
                }
            });
        }
    }
}

const REFRESH_FAILED: &str = "Some providers could not refresh";
const REFRESH_TIMEOUT: Duration = Duration::from_secs(60);

fn refresh_message(results: &[RefreshResult]) -> Option<String> {
    let any = |state: RefreshState| results.iter().any(|result| result.state == state);
    if any(RefreshState::Failed) {
        Some(REFRESH_FAILED.to_string())
    } else if any(RefreshState::Pending) {
        Some("Refresh still in progress".to_string())
    } else if any(RefreshState::NotFound) {
        Some("Provider no longer available".to_string())
    } else {
        None
    }
}

async fn execute_remote(
    app: &AppHandle, command: Command, ticket: Option<Ticket>,
) -> Result<(), crate::commands::ErrorResponse> {
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        if !inner.active() || ticket.is_some_and(|ticket| !inner.current(ticket)) {
            return Ok(());
        }
        if matches!(command, Command::Refresh) {
            if inner.refresh_active {
                return Ok(());
            }
            inner.refresh_active = true;
        }
    }
    let remote = app.state::<RemoteConnection>();
    match command {
        Command::Unpin(pipeline_id) => {
            let body = serde_json::json!({ "pipeline_id": pipeline_id, "member": false });
            let (status, _) = client_request(
                &remote,
                "PUT",
                "/pipeline-views/pinned/membership",
                Some(&body.to_string()),
            )
            .await?;
            if status != 200 {
                return Err(super::failure("The server couldn't unpin the pipeline"));
            }
        }
        Command::Refresh => {
            let answer =
                client_request_within(&remote, "POST", "/refresh/all", None, REFRESH_TIMEOUT).await;
            let message = match answer {
                Ok((200, body)) => serde_json::from_str::<Vec<RefreshResult>>(&body).map_or_else(
                    |_| Some(REFRESH_FAILED.to_string()),
                    |r| refresh_message(&r),
                ),
                _ => Some(REFRESH_FAILED.to_string()),
            };
            let state = app.state::<DesktopState>();
            let mut inner = state.lock();
            inner.refresh_active = false;
            inner.refresh_message = message;
        }
        _ => {}
    }
    super::wake(app);
    Ok(())
}

async fn execute(
    app: &AppHandle, command: Command, ticket: Option<Ticket>,
) -> Result<(), crate::commands::ErrorResponse> {
    if let Command::Notifications = command {
        let state = app.state::<DesktopState>();
        let _guard = state.settings_gate.lock().await;
        let (next, request) = {
            let inner = state.lock();
            if ticket.is_some_and(|ticket| !inner.current(ticket)) {
                return Ok(());
            }
            let request = DesktopSettingsPatch {
                notifications_enabled: Some(!inner.saved.settings.notifications_enabled),
                ..Default::default()
            };
            (inner.saved.patch(request)?, request)
        };
        return super::commit_settings(app, next, request).await.map(|_| ());
    }
    if app.state::<DesktopState>().lock().client.is_some() {
        return execute_remote(app, command, ticket).await;
    }
    let (core, current) = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        if !inner.vault_ready || ticket.is_some_and(|ticket| !inner.matches(ticket)) {
            return Ok(());
        }
        let core = inner
            .core
            .clone()
            .ok_or_else(|| super::failure("Pipedash is not ready"))?;
        if matches!(command, Command::Refresh) {
            if inner.refresh_active {
                return Ok(());
            }
            inner.refresh_active = true;
        }
        (core, inner.ticket())
    };
    match command {
        Command::Unpin(pipeline_id) => {
            let lock = app.state::<crate::app_lock::AppLockState>();
            let _transition = lock.transition().await;
            if !app.state::<DesktopState>().lock().matches(current) {
                return Ok(());
            }
            core.pipeline_view_service
                .set_membership(ViewRef::Pinned, &pipeline_id, false)
                .await?;
        }
        Command::StopWatch(session_id) => {
            crate::build_watch::stop_matching(app, &session_id).await?;
        }
        Command::Lock => {
            crate::app_lock::lock_session(app).await;
        }
        Command::Refresh => {
            super::wake(app);
            let result = core.pipeline_service.refresh_all().await;
            let state = app.state::<DesktopState>();
            let mut inner = state.lock();
            if !inner.matches(current) {
                return Ok(());
            }
            inner.refresh_active = false;
            inner.refresh_message = result.map_or_else(
                |_| Some(REFRESH_FAILED.to_string()),
                |results| refresh_message(&results),
            );
        }
        _ => {}
    }
    super::wake(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        key: &str, tone: &'static str, pipeline: Option<(&str, Option<i64>)>,
    ) -> DesktopNotice {
        DesktopNotice {
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
        inner.history.push(notice("old", "success", None));
        inner.history.mark_read(None);
        inner
            .history
            .push(notice("a", "danger", Some(("acme/api", Some(42)))));
        inner
            .history
            .push(notice("b", "warning", Some(("acme/web", None))));
        inner.history.push(notice("c", "success", None));
        inner.history.push(notice("d", "danger", None));

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
        inner
            .history
            .push(notice("run", "danger", Some(("acme/api", Some(42)))));

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
        inner.history.push(notice("seen", "danger", None));
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
                    provider_type: "jenkins".to_string(),
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
            status: crate::remote::RemoteStatus::Connected,
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
            status: crate::remote::RemoteStatus::Reconnecting,
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
            status: crate::remote::RemoteStatus::Removed,
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
}
