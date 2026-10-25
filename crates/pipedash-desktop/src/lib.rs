mod app_lock;
mod build_watch;
mod commands;
mod core_boot;
mod credential;
mod export_file;
pub mod fallback_store;
pub mod keyring_store;
mod native;
mod push;
mod system_notifications;
pub mod tauri_event_bus;

use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use pipedash_core::infrastructure::config::{
    ConfigLoader,
    PipedashConfig,
};
use pipedash_core::CoreContext;
use tauri::Manager;

#[cfg(target_os = "macos")]
fn install_app_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{
        Menu,
        MenuItem,
        MenuItemKind,
        Submenu,
    };
    use tauri::Emitter;

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
        if matches!(
            command,
            "refresh-all" | "toggle-sidebar" | "settings" | "lock" | "find"
        ) {
            if let Err(error) = app.emit("menu-command", command) {
                tracing::error!("Could not dispatch menu command: {}", error);
            }
        }
    });
    Ok(())
}

fn ensure_desktop_config(app_data_dir: &Path, config_path: &Path) -> Result<bool, String> {
    if config_path.exists() {
        return Ok(false);
    }

    let legacy_db_path = app_data_dir.join("pipedash.db");

    let is_legacy_migration = if legacy_db_path.exists() {
        let metadata = std::fs::metadata(&legacy_db_path)
            .map_err(|e| format!("Failed to check legacy database: {}", e))?;

        if metadata.len() > 0 {
            tracing::info!(
                "Detected legacy database at {} ({} bytes) - preserving existing data",
                legacy_db_path.display(),
                metadata.len()
            );
            true
        } else {
            tracing::debug!("Legacy database exists but is empty - treating as fresh install");
            false
        }
    } else {
        false
    };

    let mut config = PipedashConfig::default();
    config.general.default_refresh_interval = 300;
    config.storage.data_dir = app_data_dir.display().to_string();

    let config_toml =
        ConfigLoader::to_toml(&config).map_err(|e| format!("Failed to serialize config: {}", e))?;

    let (title, note) = if is_legacy_migration {
        (
            "Auto-generated configuration for legacy database",
            "This config was automatically created to maintain backward compatibility",
        )
    } else {
        (
            "Pipedash Desktop Configuration",
            "Desktop uses SQLite with system keyring for secure credential storage",
        )
    };

    let config_content = format!(
        "# {}\n# Created: {}\n# {}\n\n{}",
        title,
        chrono::Utc::now().to_rfc3339(),
        note,
        config_toml
    );

    std::fs::write(config_path, &config_content)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    if is_legacy_migration {
        tracing::info!(
            "Successfully created config.toml for legacy database at {}",
            config_path.display()
        );
        tracing::info!("Existing database and credentials will be preserved");
    } else {
        tracing::info!(
            "Successfully created default config.toml at {}",
            config_path.display()
        );
        tracing::info!("Desktop initialized with SQLite + System Keyring (default)");
    }

    Ok(true)
}

#[cfg(any(mobile, test))]
fn follow_app_container(config_path: &Path) -> Result<bool, String> {
    if !config_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let mut config =
        ConfigLoader::parse_raw(&content).map_err(|e| format!("Failed to parse config: {}", e))?;
    if config.storage.data_dir.is_empty() {
        return Ok(false);
    }

    config.storage.data_dir.clear();
    ConfigLoader::save(&config, config_path)
        .map_err(|e| format!("Failed to save config: {}", e))?;
    Ok(true)
}

pub struct AppDataDir(pub PathBuf);

#[derive(Default)]
pub struct MaybeCoreContext(pub Arc<tokio::sync::RwLock<Option<Arc<CoreContext>>>>);

impl MaybeCoreContext {
    pub async fn get(&self) -> Result<Arc<CoreContext>, String> {
        self.0
            .read()
            .await
            .clone()
            .ok_or_else(|| "Application not initialized. Please complete setup first.".to_string())
    }
}

use app_lock::{
    set_app_lock,
    unlock_with_biometrics,
    AppLockState,
};
use build_watch::{
    get_build_watch,
    start_build_watch,
    stop_build_watch,
    BuildWatchState,
};
use commands::{
    add_provider,
    analyze_config,
    bootstrap_app,
    cancel_pipeline_run,
    check_database_exists,
    check_provider_permissions,
    check_setup_status,
    clear_all_caches,
    clear_all_run_history_caches,
    clear_pipelines_cache,
    clear_run_history_cache,
    clear_workflow_params_cache,
    create_initial_config,
    execute_storage_migration,
    export_providers,
    factory_reset,
    fetch_provider_organizations,
    fetch_run_history,
    flush_pipeline_metrics,
    get_available_plugins,
    get_cache_stats,
    get_cached_pipelines,
    get_config_content,
    get_default_data_dir,
    get_default_table_preferences,
    get_effective_data_dir,
    get_global_metrics_config,
    get_metrics_storage_stats,
    get_pipeline_metrics_config,
    get_provider,
    get_provider_features,
    get_provider_field_options,
    get_provider_permissions,
    get_provider_table_schema,
    get_refresh_mode,
    get_storage_config,
    get_storage_paths,
    get_table_preferences,
    get_vault_password_status,
    get_vault_status,
    get_workflow_parameters,
    get_workflow_run_details,
    import_providers,
    list_plugin_metadata,
    list_providers,
    lock_vault,
    plan_storage_migration,
    preview_provider_pipelines,
    query_aggregated_metrics,
    query_pipeline_metrics,
    refresh_all,
    refresh_provider,
    remove_provider,
    rerun_failed_jobs,
    reset_metrics_processing_state,
    restart_app,
    review_pipeline_run,
    save_config_content,
    save_storage_config,
    save_table_preferences,
    set_refresh_mode,
    test_storage_connection,
    trigger_pipeline,
    unlock_vault,
    update_general_settings,
    update_global_metrics_config,
    update_pipeline_metrics_config,
    update_provider,
    update_provider_refresh_interval,
    validate_provider_credentials,
    validate_storage_config,
};
use core_boot::BootKind;
use export_file::save_export_file;
use push::{
    get_push_status,
    push_pair_server,
    push_send_test,
    push_unpair_server,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    pipedash_core::logging::init();

    if let Err(e) = credential::init() {
        tracing::error!("Failed to initialize the credential store: {}", e);
    }

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_pipedash_push::init())
        .plugin(tauri_plugin_pipedash_native::init())
        .manage(native::PendingIntent::default())
        .manage(BuildWatchState::default());

    #[cfg(mobile)]
    let builder = builder
        .plugin(tauri_plugin_biometric::init())
        .on_window_event(app_lock::handle_window_event);

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            install_app_menu(app.handle())?;
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            app.remove_menu()?;

            let app_data_dir = match app.path().app_data_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    tracing::error!("Failed to get app data directory: {}. Using fallback.", e);
                    dirs::data_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("pipedash")
                }
            };

            if let Err(e) = std::fs::create_dir_all(&app_data_dir) {
                tracing::error!("Failed to create app data directory: {}", e);
                return Err(format!(
                    "Cannot create data directory: {}. Please check permissions.",
                    e
                )
                .into());
            }

            let config_path = app_data_dir.join("config.toml");
            std::env::set_var("PIPEDASH_CONFIG_PATH", config_path.as_os_str());
            tracing::info!("Set PIPEDASH_CONFIG_PATH to: {}", config_path.display());

            match ensure_desktop_config(&app_data_dir, &config_path) {
                Ok(true) => {
                    tracing::info!("Desktop config initialized - proceeding with normal startup");
                }
                Ok(false) => {
                    tracing::debug!("Config already exists - using existing configuration");
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to ensure desktop config: {}. This is unexpected for desktop.",
                        e
                    );
                }
            }

            #[cfg(mobile)]
            {
                std::env::set_var("PIPEDASH_DATA_DIR", app_data_dir.as_os_str());
                match follow_app_container(&config_path) {
                    Ok(true) => tracing::info!("Storage now follows the app container"),
                    Ok(false) => {}
                    Err(e) => tracing::error!("Failed to update storage location: {}", e),
                }
            }

            let setup_complete = config_path.exists();
            let app_lock = AppLockState::load(&app_data_dir, app_lock::SUPPORTED, setup_complete);
            let start_locked = app_lock.locked();
            let lock_settings = app_lock.settings();

            app.manage(AppDataDir(app_data_dir.clone()));
            app.manage(MaybeCoreContext::default());
            app.manage(app_lock);
            native::sync_privacy_cover(app.handle(), lock_settings);

            if !setup_complete {
                tracing::info!("No configuration found - starting in setup mode");
                tracing::info!("Setup wizard will be shown to complete initial configuration");
            } else if start_locked {
                tracing::info!("App lock enabled - starting locked; the core loads after unlock");
            } else {
                tracing::info!("Configuration found - initializing application");

                tauri::async_runtime::block_on(core_boot::boot_core(
                    app.handle(),
                    BootKind::Launch,
                    false,
                ))
                .map_err(|e| {
                    tracing::error!("CoreContext initialization failed: {}", e);
                    e
                })?;
            }

            system_notifications::register(app.handle())?;
            build_watch::register(app.handle())?;

            push::spawn_token_sync(app.handle(), &app_data_dir);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_setup_status,
            create_initial_config,
            bootstrap_app,
            add_provider,
            list_providers,
            get_provider,
            update_provider,
            update_provider_refresh_interval,
            remove_provider,
            export_providers,
            import_providers,
            get_available_plugins,
            list_plugin_metadata,
            get_provider_field_options,
            fetch_provider_organizations,
            preview_provider_pipelines,
            validate_provider_credentials,
            check_provider_permissions,
            get_provider_permissions,
            get_provider_features,
            get_provider_table_schema,
            get_cached_pipelines,
            fetch_run_history,
            get_workflow_run_details,
            get_build_watch,
            start_build_watch,
            stop_build_watch,
            trigger_pipeline,
            cancel_pipeline_run,
            review_pipeline_run,
            rerun_failed_jobs,
            get_workflow_parameters,
            refresh_all,
            refresh_provider,
            set_refresh_mode,
            get_refresh_mode,
            update_general_settings,
            clear_run_history_cache,
            get_cache_stats,
            clear_pipelines_cache,
            clear_all_run_history_caches,
            clear_workflow_params_cache,
            clear_all_caches,
            get_global_metrics_config,
            update_global_metrics_config,
            get_pipeline_metrics_config,
            update_pipeline_metrics_config,
            query_pipeline_metrics,
            query_aggregated_metrics,
            get_metrics_storage_stats,
            flush_pipeline_metrics,
            reset_metrics_processing_state,
            get_table_preferences,
            save_table_preferences,
            get_default_table_preferences,
            get_storage_config,
            get_vault_password_status,
            get_vault_status,
            unlock_vault,
            lock_vault,
            save_storage_config,
            get_config_content,
            save_config_content,
            analyze_config,
            get_storage_paths,
            get_default_data_dir,
            get_effective_data_dir,
            check_database_exists,
            test_storage_connection,
            validate_storage_config,
            plan_storage_migration,
            execute_storage_migration,
            factory_reset,
            restart_app,
            unlock_with_biometrics,
            set_app_lock,
            save_export_file,
            get_push_status,
            push_pair_server,
            push_unpair_server,
            push_send_test,
            system_notifications::show_system_notification,
            native::native_haptic,
            native::native_set_appearance,
            native::native_accessibility,
            native::native_open_settings,
            native::native_share_link,
            native::native_set_badge,
            native::native_release_privacy_cover,
            native::native_take_pending_intent,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| native::on_run_event(app, &event));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_generated_config_loads(with_legacy_db: bool) {
        let root = tempfile::tempdir().unwrap();
        let app_data_dir = if cfg!(windows) {
            root.path().join("com.henrique.pipedash")
        } else {
            root.path()
                .join(r"C:\Users\alice\AppData\Roaming\com.henrique.pipedash")
        };
        std::fs::create_dir_all(&app_data_dir).unwrap();
        if with_legacy_db {
            std::fs::write(app_data_dir.join("pipedash.db"), b"legacy").unwrap();
        }
        let config_path = app_data_dir.join("config.toml");

        assert_eq!(ensure_desktop_config(&app_data_dir, &config_path), Ok(true));

        let config = ConfigLoader::load(&config_path).expect("generated config.toml must load");
        assert_eq!(config.data_dir(), app_data_dir);
        assert!(config.storage.backend.is_sqlite());
        assert_eq!(config.general.default_refresh_interval, 300);
    }

    #[test]
    fn fresh_config_loads_from_a_windows_style_data_dir() {
        assert_generated_config_loads(false);
    }

    #[test]
    fn legacy_config_loads_from_a_windows_style_data_dir() {
        assert_generated_config_loads(true);
    }

    #[test]
    fn the_desktop_config_templates_turn_status_pages_on() {
        let root =
            std::env::temp_dir().join(format!("pipedash-desktop-config-{}", std::process::id()));
        for legacy in [false, true] {
            let data_dir = root.join(if legacy { "legacy" } else { "fresh" });
            std::fs::create_dir_all(&data_dir).unwrap();
            if legacy {
                std::fs::write(data_dir.join("pipedash.db"), b"data").unwrap();
            }
            let config_path = data_dir.join("config.toml");

            assert!(ensure_desktop_config(&data_dir, &config_path).unwrap());

            let text = std::fs::read_to_string(&config_path).unwrap();
            assert!(text.contains("status_pages_enabled = true"), "{text}");
            let config = ConfigLoader::load(&config_path).unwrap();
            assert!(config.general.status_pages_enabled);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_config_from_an_old_app_container_follows_the_current_one() {
        let root = tempfile::tempdir().unwrap();
        let config_path = root.path().join("config.toml");
        std::fs::write(
            &config_path,
            "[general]\nmetrics_enabled = true\n\n[storage]\n\
             data_dir = \"/var/mobile/Containers/Data/Application/OLD-UUID/Library/Application Support/com.henrique.pipedash\"\n\
             backend = \"sqlite\"\n\n[providers.github]\ntype = \"github\"\ntoken = \"${GITHUB_TOKEN}\"\n",
        )
        .unwrap();

        assert_eq!(follow_app_container(&config_path), Ok(true));

        let text = std::fs::read_to_string(&config_path).unwrap();
        assert!(!text.contains("OLD-UUID"), "{text}");
        assert!(
            text.contains("${GITHUB_TOKEN}"),
            "env references must stay unexpanded: {text}"
        );
        let config = ConfigLoader::parse_raw(&text).unwrap();
        assert!(config.storage.data_dir.is_empty());
        assert!(config.storage.backend.is_sqlite());
        assert_eq!(follow_app_container(&config_path), Ok(false));
    }
}
