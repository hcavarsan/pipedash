use tauri::menu::{
    Menu,
    MenuItem,
    MenuItemKind,
    Submenu,
};
use tauri::{
    AppHandle,
    Emitter,
};

pub(crate) fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::default(app)?;
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        app_menu.insert(
            &MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?,
            2,
        )?;
        app_menu.insert(
            &MenuItem::with_id(app, "lock", "Lock", true, Some("CmdOrCtrl+L"))?,
            3,
        )?;
    }
    let pipelines = Submenu::with_items(
        app,
        "Pipelines",
        true,
        &[
            &MenuItem::with_id(app, "refresh-all", "Refresh All", true, Some("CmdOrCtrl+R"))?,
            &MenuItem::with_id(
                app,
                "toggle-sidebar",
                "Toggle Sidebar",
                true,
                Some("CmdOrCtrl+\\"),
            )?,
            &MenuItem::with_id(app, "find", "Find", true, Some("CmdOrCtrl+F"))?,
        ],
    )?;
    menu.insert(&pipelines, 3)?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let command = event.id().as_ref();
        match command {
            "settings" | "lock" | "refresh-all" => crate::desktop::dispatch_app_menu(app, command),
            "toggle-sidebar" | "find" => {
                if let Err(error) = app.emit("menu-command", command) {
                    tracing::error!(%error, "Could not dispatch menu command");
                }
            }
            _ => {}
        }
    });
    Ok(())
}
