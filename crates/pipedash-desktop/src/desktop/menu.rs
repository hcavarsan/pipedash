pub(super) mod actions;
mod model;
mod reconcile;
#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        PoisonError,
    },
};

use tauri::{
    image::Image,
    menu::Menu,
    tray::TrayIconBuilder,
    AppHandle,
    Manager,
    Runtime,
};

use self::{
    actions::dispatch,
    reconcile::{
        reconcile,
        Container,
        MenuOwner,
        MenuState,
    },
};
use super::{
    state::{
        preferences,
        publish_status,
        DesktopState,
    },
    tray::{
        self,
        TrayStatus,
    },
};

const TRAY_ID: &str = "pipedash-main";

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
            publish_status(&handle);
        }
    }) {
        tracing::warn!(%error, "Couldn't schedule the native menu");
    }
}

pub(super) fn apply(app: &AppHandle) -> tauri::Result<()> {
    let prefs = preferences(app);
    let (rows, ticket, look) = {
        let state = app.state::<DesktopState>();
        let inner = state.lock();
        let look = Look {
            status: tray::tray_status(&inner, &prefs),
            title: tray::title(&inner),
            tooltip: tray::tooltip(&inner, &prefs),
        };
        (model::model(&inner, &prefs), inner.ticket(), look)
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
