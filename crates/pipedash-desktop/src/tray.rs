//! System tray (menu bar) implementation for Pipedash.
//!
//! Displays pipeline status in the macOS menu bar and provides quick access
//! to pinned pipelines.

use std::sync::Arc;

use pipedash_core::tray_status::{TrayStatus, TrayStatusSummary};
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};
use tokio::sync::RwLock;

/// Manages the system tray icon and menu
pub struct TrayManager {
    tray_icon: TrayIcon<Wry>,
    current_status: Arc<RwLock<TrayStatus>>,
}

impl TrayManager {
    /// Create a new tray manager and initialize the tray icon
    pub fn new(app: &AppHandle<Wry>) -> Result<Self, Box<dyn std::error::Error>> {
        // Create initial menu items
        let open_item = MenuItemBuilder::with_id("open", "Open Pipedash")
            .build(app)?;
        let separator = PredefinedMenuItem::separator(app)?;
        let no_pinned = MenuItemBuilder::with_id("no_pinned", "No pinned pipelines")
            .enabled(false)
            .build(app)?;
        let quit_item = MenuItemBuilder::with_id("quit", "Quit Pipedash")
            .build(app)?;

        // Build menu
        let menu = MenuBuilder::new(app)
            .item(&no_pinned)
            .item(&separator)
            .item(&open_item)
            .item(&quit_item)
            .build()?;

        // Get default icon from app resources
        let icon = Self::load_icon_for_status(TrayStatus::Unknown)?;

        // Build tray icon
        let tray_icon = TrayIconBuilder::with_id("pipedash-tray")
            .icon(icon)
            .menu(&menu)
            .show_menu_on_left_click(true)
            .tooltip("Pipedash")
            .on_menu_event(move |app, event| {
                match event.id.as_ref() {
                    "open" => {
                        // Focus or create the main window
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    id if id.starts_with("pipeline:") => {
                        // Pipeline clicked - open app and navigate to it
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            // TODO: Emit event to navigate to specific pipeline
                        }
                    }
                    _ => {}
                }
            })
            .build(app)?;

        Ok(Self {
            tray_icon,
            current_status: Arc::new(RwLock::new(TrayStatus::Unknown)),
        })
    }

    /// Load the appropriate icon for a given status
    fn load_icon_for_status(status: TrayStatus) -> Result<Image<'static>, Box<dyn std::error::Error>> {
        // For now, use a simple colored dot based on status
        // In a real implementation, you'd load actual PNG files
        let icon_bytes: &[u8] = match status {
            TrayStatus::Passed => include_bytes!("../icons/tray-passed.png"),
            TrayStatus::Failed => include_bytes!("../icons/tray-failed.png"),
            TrayStatus::Running => include_bytes!("../icons/tray-running.png"),
            TrayStatus::Cancelled => include_bytes!("../icons/tray-cancelled.png"),
            TrayStatus::Unknown => include_bytes!("../icons/tray-idle.png"),
        };

        Ok(Image::from_bytes(icon_bytes)?)
    }

    /// Update the tray with the current pipeline status
    pub async fn update_status(
        &self,
        app: &AppHandle<Wry>,
        summary: &TrayStatusSummary,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Update icon based on overall status
        let new_status = summary.overall_status;

        {
            let mut current = self.current_status.write().await;
            if *current == new_status {
                // Status unchanged, only update menu if pipelines changed
            }
            *current = new_status;
        }

        // Update icon
        let icon = Self::load_icon_for_status(new_status)?;
        self.tray_icon.set_icon(Some(icon))?;

        // Update tooltip
        let tooltip = if summary.has_pinned_pipelines() {
            format!(
                "Pipedash: {} pinned ({} passed, {} failed, {} running)",
                summary.total_count,
                summary.passed_count,
                summary.failed_count,
                summary.running_count
            )
        } else {
            "Pipedash: No pinned pipelines".to_string()
        };
        self.tray_icon.set_tooltip(Some(&tooltip))?;

        // Rebuild menu with current pipelines
        self.rebuild_menu(app, summary)?;

        Ok(())
    }

    /// Rebuild the tray menu with current pipeline status
    fn rebuild_menu(
        &self,
        app: &AppHandle<Wry>,
        summary: &TrayStatusSummary,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut menu_builder = MenuBuilder::new(app);

        if summary.has_pinned_pipelines() {
            // Add each pinned pipeline
            for pipeline in &summary.pinned_pipelines {
                let status_emoji = match pipeline.status {
                    pipedash_core::PipelineStatus::Success => "\u{2705}", // Green checkmark
                    pipedash_core::PipelineStatus::Failed => "\u{274C}",  // Red X
                    pipedash_core::PipelineStatus::Running => "\u{1F504}", // Rotating arrows
                    pipedash_core::PipelineStatus::Pending => "\u{23F3}",  // Hourglass
                    pipedash_core::PipelineStatus::Cancelled => "\u{26D4}", // No entry
                    pipedash_core::PipelineStatus::Skipped => "\u{23ED}", // Skip
                };

                // Format: {owner/repo} - {workflow name} {status icon}
                let label = format!("{} - {} {}", pipeline.repository, pipeline.name, status_emoji);
                let item_id = format!("pipeline:{}", pipeline.id);

                let item = MenuItemBuilder::with_id(&item_id, &label)
                    .build(app)?;
                menu_builder = menu_builder.item(&item);
            }
        } else {
            let no_pinned = MenuItemBuilder::with_id("no_pinned", "No pinned pipelines")
                .enabled(false)
                .build(app)?;
            menu_builder = menu_builder.item(&no_pinned);
        }

        // Add separator and standard items
        let separator = PredefinedMenuItem::separator(app)?;
        let open_item = MenuItemBuilder::with_id("open", "Open Pipedash")
            .build(app)?;
        let quit_item = MenuItemBuilder::with_id("quit", "Quit Pipedash")
            .build(app)?;

        let menu = menu_builder
            .item(&separator)
            .item(&open_item)
            .item(&quit_item)
            .build()?;

        self.tray_icon.set_menu(Some(menu))?;

        Ok(())
    }

    /// Get a reference to the underlying tray icon
    pub fn tray_icon(&self) -> &TrayIcon<Wry> {
        &self.tray_icon
    }
}

/// Initialize the system tray for the app
pub fn setup_tray(app: &AppHandle<Wry>) -> Result<TrayManager, Box<dyn std::error::Error>> {
    TrayManager::new(app)
}
