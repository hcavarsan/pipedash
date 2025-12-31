mod commands;
pub mod fallback_store;
pub mod keyring_store;
pub mod tauri_event_bus;

use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use pipedash_core::CoreContext;
use tauri::Manager;

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

    let config_content = if is_legacy_migration {
        format!(
            r#"# Auto-generated configuration for legacy database
# Created: {}
# This config was automatically created to maintain backward compatibility

[general]
metrics_enabled = true
default_refresh_interval = 300

[server]
bind_addr = "127.0.0.1:8080"
cors_allow_all = true

[storage]
data_dir = "{}"
backend = "sqlite"
"#,
            chrono::Utc::now().to_rfc3339(),
            app_data_dir.display()
        )
    } else {
        format!(
            r#"# Pipedash Desktop Configuration
# Created: {}
# Desktop uses SQLite with system keyring for secure credential storage

[general]
metrics_enabled = true
default_refresh_interval = 300

[server]
bind_addr = "127.0.0.1:8080"
cors_allow_all = true

[storage]
data_dir = "{}"
backend = "sqlite"
"#,
            chrono::Utc::now().to_rfc3339(),
            app_data_dir.display()
        )
    };

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

#[derive(Default)]
struct BackupInfo {
    main_db_backup: Option<PathBuf>,
    main_db_original: Option<PathBuf>,
    metrics_db_backup: Option<PathBuf>,
    metrics_db_original: Option<PathBuf>,
}

fn is_database_corruption_error(error_msg: &str) -> bool {
    let msg = error_msg.to_lowercase();
    msg.contains("corrupt")
        || msg.contains("malformed")
        || msg.contains("database disk image")
        || msg.contains("file is not a database")
        || msg.contains("database is locked") && msg.contains("timeout")
        || msg.contains("unable to open database")
}

fn reset_corrupted_databases(app_data_dir: &Path) -> Result<BackupInfo, String> {
    let db_path = app_data_dir.join("pipedash.db");
    let metrics_db_path = app_data_dir.join("metrics.db");

    let backup_dir = app_data_dir.join("corrupt_backups");
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let mut backup_info = BackupInfo::default();

    if db_path.exists() {
        let backup_name = format!("pipedash_{}.db.corrupt", timestamp);
        let backup_path = backup_dir.join(&backup_name);
        if let Err(e) = std::fs::rename(&db_path, &backup_path) {
            tracing::warn!("Could not backup main db: {}. Removing directly.", e);
            std::fs::remove_file(&db_path)
                .map_err(|e| format!("Failed to remove corrupted main db: {}", e))?;
        } else {
            tracing::info!("Backed up main db to: {}", backup_name);
            backup_info.main_db_backup = Some(backup_path);
            backup_info.main_db_original = Some(db_path.clone());
        }
    }

    if metrics_db_path.exists() {
        let backup_name = format!("metrics_{}.db.corrupt", timestamp);
        let backup_path = backup_dir.join(&backup_name);
        if let Err(e) = std::fs::rename(&metrics_db_path, &backup_path) {
            tracing::warn!("Could not backup metrics db: {}. Removing directly.", e);
            std::fs::remove_file(&metrics_db_path)
                .map_err(|e| format!("Failed to remove corrupted metrics db: {}", e))?;
        } else {
            tracing::info!("Backed up metrics db to: {}", backup_name);
            backup_info.metrics_db_backup = Some(backup_path);
            backup_info.metrics_db_original = Some(metrics_db_path.clone());
        }
    }

    Ok(backup_info)
}

fn restore_database_backups(backup_info: &BackupInfo) -> Result<(), String> {
    let mut restored = false;

    if let (Some(backup), Some(original)) =
        (&backup_info.main_db_backup, &backup_info.main_db_original)
    {
        if backup.exists() && !original.exists() {
            std::fs::rename(backup, original)
                .map_err(|e| format!("Failed to restore main db from backup: {}", e))?;
            tracing::info!("Restored main database from backup");
            restored = true;
        }
    }

    if let (Some(backup), Some(original)) = (
        &backup_info.metrics_db_backup,
        &backup_info.metrics_db_original,
    ) {
        if backup.exists() && !original.exists() {
            std::fs::rename(backup, original)
                .map_err(|e| format!("Failed to restore metrics db from backup: {}", e))?;
            tracing::info!("Restored metrics database from backup");
            restored = true;
        }
    }

    if restored {
        tracing::info!("Databases restored to original state");
    }

    Ok(())
}

pub struct AppDataDir(pub PathBuf);

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

use commands::{
    add_provider,
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
    factory_reset,
    fetch_pipelines,
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
    list_plugin_metadata,
    list_providers,
    lock_vault,
    plan_storage_migration,
    preview_provider_pipelines,
    query_aggregated_metrics,
    query_pipeline_metrics,
    refresh_all,
    remove_provider,
    reset_metrics_processing_state,
    restart_app,
    save_config_content,
    save_storage_config,
    save_table_preferences,
    set_refresh_mode,
    test_storage_connection,
    trigger_pipeline,
    unlock_vault,
    update_global_metrics_config,
    update_pipeline_metrics_config,
    update_provider,
    update_provider_refresh_interval,
    validate_provider_credentials,
    validate_storage_config,
};
use fallback_store::FallbackTokenStore;
use keyring_store::KeyringTokenStore;
use tauri_event_bus::create_tauri_event_bus;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    pipedash_core::logging::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
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
                return Err(format!("Cannot create data directory: {}. Please check permissions.", e).into());
            }

            let config_path = app_data_dir.join("config.toml");
            std::env::set_var("PIPEDASH_CONFIG_PATH", config_path.as_os_str());
            tracing::info!(
                "Set PIPEDASH_CONFIG_PATH to: {}",
                config_path.display()
            );

            use pipedash_core::infrastructure::config::ConfigLoader;

            let maybe_core_context = Arc::new(tokio::sync::RwLock::new(None));

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

            if config_path.exists() {
                tracing::info!("Configuration found - initializing application");

                let core_context = tauri::async_runtime::block_on(async {
                    let config = ConfigLoader::load(&config_path).map_err(|e| {
                        format!("Failed to load configuration: {}", e)
                    })?;

                    let event_bus = create_tauri_event_bus(app.handle().clone());

                    let vault_password_available = std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok();

                    let db_path = config.data_dir().join("pipedash.db");
                    let has_encrypted_tokens =
                        pipedash_core::infrastructure::database::has_encrypted_tokens(&db_path).await;

                    let use_keyring = config.storage.backend.is_sqlite()
                        && !vault_password_available
                        && !has_encrypted_tokens;

                    let use_fallback_store = config.storage.backend.is_sqlite() && vault_password_available;

                    let vault_locked = if config.storage.backend.is_sqlite() {
                        has_encrypted_tokens && !vault_password_available
                    } else {
                        !vault_password_available
                    };

                    if use_keyring {
                        tracing::info!("Using system keyring for credential storage (desktop SQLite default - no encrypted tokens found)");
                    } else if vault_locked {
                        tracing::info!(
                            "Vault is locked: encrypted tokens found but no password provided. \
                            Starting in locked mode - UI will prompt for unlock."
                        );
                    } else if use_fallback_store {
                        tracing::info!(
                            "Using encrypted database with keyring fallback for credential storage \
                            (migrating tokens from keyring to encrypted storage)"
                        );
                    } else if config.storage.backend.is_sqlite() {
                        tracing::info!("Using encrypted database for credential storage (vault password detected)");
                    } else {
                        tracing::info!("Using encrypted database for credential storage (PostgreSQL backend)");
                    }

                    let create_storage_manager = || async {
                        if use_keyring {
                            let token_store: Arc<dyn pipedash_core::infrastructure::TokenStore> =
                                Arc::new(KeyringTokenStore::new());
                            pipedash_core::infrastructure::StorageManager::with_token_store(
                                config.clone(),
                                token_store,
                                true
                            ).await
                        } else if vault_locked {
                            tracing::warn!(
                                "Starting with locked vault - tokens will be inaccessible until unlocked"
                            );
                            let token_store: Arc<dyn pipedash_core::infrastructure::TokenStore> =
                                Arc::new(pipedash_core::infrastructure::secrets::MemoryTokenStore::new());
                            pipedash_core::infrastructure::StorageManager::with_token_store_locked(
                                config.clone(),
                                token_store,
                                true  // Desktop mode
                            ).await
                        } else if use_fallback_store {
                            use pipedash_core::domain::DomainError;
                            use pipedash_core::infrastructure::database::init_database;
                            use pipedash_core::infrastructure::secrets::SqliteTokenStore;

                            let data_dir = config.data_dir();
                            let db_path = data_dir.join("pipedash.db");

                            let pool = init_database(db_path).await.map_err(|e| {
                                DomainError::DatabaseError(format!(
                                    "Failed to initialize SQLite database: {}",
                                    e
                                ))
                            })?;

                            let primary_store: Arc<dyn pipedash_core::infrastructure::TokenStore> =
                                Arc::new(SqliteTokenStore::new(pool, None).await?);

                            let fallback_store: Arc<dyn pipedash_core::infrastructure::TokenStore> =
                                Arc::new(KeyringTokenStore::new());

                            let token_store: Arc<dyn pipedash_core::infrastructure::TokenStore> =
                                Arc::new(FallbackTokenStore::new(primary_store, fallback_store));

                            tracing::info!(
                                "Created FallbackTokenStore: encrypted SQLite (primary) + keyring (fallback)"
                            );

                            pipedash_core::infrastructure::StorageManager::with_token_store(
                                config.clone(),
                                token_store,
                                true  // Still desktop mode for other behaviors
                            ).await
                        } else {
                            pipedash_core::infrastructure::StorageManager::from_config_allow_locked(
                                config.clone(),
                                false  // Not using desktop keyring mode
                            ).await
                        }
                    };

                    let storage_manager = match create_storage_manager().await {
                        Ok(mgr) => mgr,
                        Err(e) => {
                            let error_msg = e.to_string();

                            if !is_database_corruption_error(&error_msg) {
                                tracing::error!(
                                    "StorageManager initialization failed (not database corruption): {}",
                                    error_msg
                                );
                                return Err(format!(
                                    "Failed to initialize storage: {}. \
                                    This appears to be a configuration issue, not database corruption. \
                                    Check your PIPEDASH_VAULT_PASSWORD environment variable.",
                                    error_msg
                                ));
                            }

                            tracing::error!(
                                "Database corruption detected: {}. Attempting recovery...",
                                error_msg
                            );

                            let backup_info = match reset_corrupted_databases(&app_data_dir) {
                                Ok(info) => info,
                                Err(reset_err) => {
                                    tracing::error!("Database backup failed: {}", reset_err);
                                    return Err(format!(
                                        "Failed to backup corrupted databases: {}. \
                                        Original error: {}. \
                                        Please try deleting the app data directory manually: {:?}",
                                        reset_err, error_msg, app_data_dir
                                    ));
                                }
                            };

                            tracing::info!("Databases backed up. Retrying initialization...");

                            match create_storage_manager().await {
                                Ok(mgr) => {
                                    tracing::info!("Recovery successful - initialized with fresh databases");
                                    mgr
                                }
                                Err(retry_err) => {
                                    tracing::error!(
                                        "Recovery failed: {}. Rolling back database backups...",
                                        retry_err
                                    );

                                    if let Err(restore_err) = restore_database_backups(&backup_info) {
                                        tracing::error!(
                                            "Failed to restore database backups: {}. \
                                            Backups are still available in corrupt_backups directory.",
                                            restore_err
                                        );
                                    }

                                    return Err(format!(
                                        "Recovery failed: {}. Databases have been restored to original state. \
                                        Please check your configuration or contact support.",
                                        retry_err
                                    ));
                                }
                            }
                        }
                    };

                    let ctx = CoreContext::with_storage_manager(&storage_manager, event_bus.clone())
                        .await
                        .map_err(|e| format!("Failed to create CoreContext: {}", e))?;
                    Ok((ctx, vault_locked))
                });

                let core_context = match core_context {
                    Ok((ctx, vault_locked)) => {
                        let ctx_arc = Arc::new(ctx);

                        let mut guard = maybe_core_context.blocking_write();
                        *guard = Some(Arc::clone(&ctx_arc));
                        drop(guard);

                        if !vault_locked {
                            let core_clone = Arc::clone(&ctx_arc);
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = core_clone.warmup_token_store().await {
                                    tracing::warn!("Token store warmup failed: {}", e);
                                }
                                core_clone.start_background_tasks().await;
                            });
                        } else {
                            tracing::info!(
                                "Vault locked - background tasks deferred until unlock. \
                                 Providers will not be loaded until vault is unlocked."
                            );
                        }

                        tracing::info!("Pipedash initialized with CoreContext");
                        ctx_arc
                    }
                    Err(e) => {
                        tracing::error!("CoreContext initialization failed: {}", e);
                        return Err(e.into());
                    }
                };

                app.manage(core_context);
            } else {
                tracing::info!("No configuration found - starting in setup mode");
                tracing::info!("Setup wizard will be shown to complete initial configuration");
            }

            app.manage(MaybeCoreContext(maybe_core_context));

            app.manage(AppDataDir(app_data_dir));

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
            fetch_pipelines,
            get_cached_pipelines,
            fetch_run_history,
            get_workflow_run_details,
            trigger_pipeline,
            cancel_pipeline_run,
            get_workflow_parameters,
            refresh_all,
            set_refresh_mode,
            get_refresh_mode,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
