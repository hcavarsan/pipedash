use std::{
    collections::HashMap,
    sync::Mutex,
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
    AppHandle,
};

use super::{
    actions::Command,
    model::Row,
    Applied,
};
use crate::desktop::state::Ticket;

pub(super) struct Node {
    key: String,
    label: String,
    enabled: bool,
    checked: Option<bool>,
    item: MenuItemKind<tauri::Wry>,
    children: Vec<Node>,
}

pub(super) struct MenuOwner {
    pub(super) menu: Menu<tauri::Wry>,
    pub(super) nodes: Vec<Node>,
    pub(super) commands: HashMap<String, (Command, Ticket)>,
    pub(super) next_id: u64,
    pub(super) applied: Applied,
}

pub(super) struct MenuState(pub(super) Mutex<MenuOwner>);

pub(super) enum Container {
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

pub(super) fn reconcile(
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
