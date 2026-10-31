use std::{
    collections::HashMap,
    sync::{
        Mutex,
        PoisonError,
    },
};

use pipedash_core::domain::{
    PinKind,
    PipelineStatus,
    RefreshState,
    ViewRef,
};
use tauri::{
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
    DesktopSettingsPatch,
    DesktopState,
    Inner,
    Ticket,
};
use crate::native::{
    self,
    NativeIntent,
    NativeUiAction,
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
}

struct MenuState(Mutex<MenuOwner>);

pub(super) fn initialize(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::new(app)?;
    app.manage(MenuState(Mutex::new(MenuOwner {
        menu,
        nodes: Vec::new(),
        commands: HashMap::new(),
        next_id: 0,
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
    #[cfg(target_os = "macos")]
    let icon = tauri::include_image!("icons/tray-template.png");
    #[cfg(not(target_os = "macos"))]
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
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
    Ok(())
}

pub(super) fn remove_tray(app: &AppHandle) -> tauri::Result<()> {
    drop(app.remove_tray_by_id(TRAY_ID));
    Ok(())
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
    let (rows, ticket) = {
        let state = app.state::<DesktopState>();
        let inner = state.lock();
        (model(&inner), inner.ticket())
    };
    let state = app.state::<MenuState>();
    let mut owner = state.0.lock().unwrap_or_else(PoisonError::into_inner);
    let MenuOwner {
        menu,
        nodes,
        commands,
        next_id,
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
    )
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

fn model(inner: &Inner) -> Vec<Row> {
    let mut rows = vec![Row::item("open", "Open Pipedash", Some(Command::Open))];
    if !inner.vault_ready {
        rows.push(Row::item(
            "readiness",
            inner
                .data_error
                .as_deref()
                .unwrap_or(if inner.generation == 0 {
                    "Open Pipedash to finish setup"
                } else {
                    "Pipedash is locked"
                }),
            None,
        ));
        rows.push(Row::section("end"));
        rows.push(Row::item("quit", "Quit Pipedash", Some(Command::Quit)));
        return rows;
    }
    if let Some(error) = inner.data_error.as_ref().or(inner.refresh_message.as_ref()) {
        rows.push(Row::item("error", error, None));
        rows.push(Row::link(
            "provider-error",
            "Open provider settings…",
            "/settings/providers",
        ));
    }
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
    rows.push(Row::link(
        "notifications",
        "Notifications",
        "/notifications",
    ));
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
    rows.push(Row::item("lock", "Lock Pipedash", Some(Command::Lock)));
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
        if !inner.vault_ready
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
        command => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let result = execute(&app, command, ticket).await;
                if result.is_err() {
                    let state = app.state::<DesktopState>();
                    let mut inner = state.lock();
                    if ticket.is_none_or(|ticket| inner.matches(ticket)) {
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

async fn execute(
    app: &AppHandle, command: Command, ticket: Option<Ticket>,
) -> Result<(), crate::commands::ErrorResponse> {
    if let Command::Notifications = command {
        let state = app.state::<DesktopState>();
        let _guard = state.settings_gate.lock().await;
        let next = {
            let inner = state.lock();
            if ticket.is_some_and(|ticket| !inner.matches(ticket)) {
                return Ok(());
            }
            inner.saved.patch(DesktopSettingsPatch {
                notifications_enabled: Some(!inner.saved.settings.notifications_enabled),
                ..Default::default()
            })?
        };
        return super::commit_settings(app, next).await.map(|_| ());
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
            inner.refresh_message = match result {
                Err(_) => Some("Some providers could not refresh".to_string()),
                Ok(results)
                    if results
                        .iter()
                        .any(|result| result.state == RefreshState::Failed) =>
                {
                    Some("Some providers could not refresh".to_string())
                }
                Ok(results)
                    if results
                        .iter()
                        .any(|result| result.state == RefreshState::Pending) =>
                {
                    Some("Refresh still in progress".to_string())
                }
                Ok(results)
                    if results
                        .iter()
                        .any(|result| result.state == RefreshState::NotFound) =>
                {
                    Some("Provider no longer available".to_string())
                }
                Ok(_) => None,
            };
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
            baseline: None,
            change_revision: 0,
            settings_error: None,
            data_error: None,
            tray_error: None,
            tray_ready: false,
            refresh_active: false,
            refresh_message: None,
            watch_session: None,
            watch: None,
            history: Default::default(),
        }
    }

    #[test]
    fn missing_pins_keep_order_and_unpin_while_overflow_and_shortcuts_remain_reachable() {
        let rows = model(&pinned_state());
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
        let rows = model(&inner);
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
        let first: Vec<_> = model(&inner)
            .into_iter()
            .filter(|row| row.key.starts_with("pipeline:"))
            .map(|row| row.key)
            .collect();
        inner.epoch += 1;
        let second: Vec<_> = model(&inner)
            .into_iter()
            .filter(|row| row.key.starts_with("pipeline:"))
            .map(|row| row.key)
            .collect();
        assert!(first.iter().all(|key| !second.contains(key)));
    }
}
