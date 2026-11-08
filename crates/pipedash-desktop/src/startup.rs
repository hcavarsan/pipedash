pub(crate) mod config;

use std::error::Error;
use std::path::{
    Path,
    PathBuf,
};

use tauri::{
    App,
    Manager,
};

use crate::app_lock::{
    self,
    AppLockSettings,
    AppLockState,
};
use crate::core_boot::background::BackgroundInit;
use crate::core_boot::{
    self,
    BootKind,
};
#[cfg(desktop)]
use crate::desktop;
use crate::remote;
use crate::remote::store::SavedMode;
use crate::{
    build_watch,
    credential,
    host,
    local_notifications,
    native,
    push,
    system_notifications,
    AppDataDir,
    MaybeCoreContext,
};

type SetupResult = Result<(), Box<dyn Error>>;

pub(crate) fn init_process() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    pipedash_core::logging::init();

    if let Err(e) = credential::init() {
        tracing::error!("Failed to initialize the credential store: {}", e);
    }

    #[cfg(target_os = "android")]
    if let Err(e) = crate::android::init_tls() {
        tracing::error!(
            "Failed to initialize the Android certificate verifier: {}",
            e
        );
    }
}

pub(crate) fn setup(app: &mut App) -> SetupResult {
    #[cfg(desktop)]
    configure_window(app)?;

    let app_data_dir = prepare_data_dir(app)?;
    let config_path = app_data_dir.join("config.toml");
    std::env::set_var("PIPEDASH_CONFIG_PATH", config_path.as_os_str());
    tracing::info!("Set PIPEDASH_CONFIG_PATH to: {}", config_path.display());
    let saved = remote::startup_mode(&app_data_dir, &config_path);
    let standalone = matches!(saved.mode, SavedMode::Standalone);

    if standalone {
        ensure_config(&app_data_dir, &config_path);
    }
    #[cfg(mobile)]
    adopt_app_container(&app_data_dir, &config_path);
    #[cfg(target_os = "android")]
    use_app_cache_dir(app);

    let setup_complete = standalone && config_path.exists();
    let app_lock = AppLockState::load(&app_data_dir, app_lock::SUPPORTED, setup_complete);
    let start_locked = app_lock.locked();
    let lock_settings = if matches!(saved.mode, SavedMode::Client { .. }) {
        AppLockSettings::default()
    } else {
        app_lock.settings()
    };

    app.manage(AppDataDir(app_data_dir.clone()));
    app.manage(MaybeCoreContext::default());
    app.manage(BackgroundInit::default());
    app.manage(app_lock);
    app.manage(remote::connection::RemoteConnection::new(saved.clone()));
    register_services(app, &app_data_dir, lock_settings)?;

    start_mode(app, &saved.mode, setup_complete, start_locked)?;

    #[cfg(mobile)]
    {
        system_notifications::register(app.handle())?;
        build_watch::register(app.handle())?;
    }

    push::spawn_legacy_cleanup(&app_data_dir);
    if standalone {
        push::spawn_sync_token(app.handle());
    }

    Ok(())
}

#[cfg(desktop)]
fn configure_window(app: &mut App) -> SetupResult {
    #[cfg(target_os = "macos")]
    crate::app_menu::install(app.handle())?;
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    app.remove_menu()?;
    if let Err(error) = desktop::fit_main_window_to_monitor(app.handle()) {
        tracing::warn!(%error, "Couldn't fit the main window to the screen");
    }
    Ok(())
}

fn prepare_data_dir(app: &App) -> Result<PathBuf, Box<dyn Error>> {
    let dir = app.path().app_data_dir().unwrap_or_else(|e| {
        tracing::error!("Failed to get app data directory: {}. Using fallback.", e);
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pipedash")
    });

    std::fs::create_dir_all(&dir).map_err(|e| {
        tracing::error!("Failed to create app data directory: {}", e);
        format!(
            "Cannot create data directory: {}. Please check permissions.",
            e
        )
    })?;
    Ok(dir)
}

fn ensure_config(app_data_dir: &Path, config_path: &Path) {
    match config::ensure_desktop_config(app_data_dir, config_path) {
        Ok(true) => tracing::info!("Desktop config initialized - proceeding with normal startup"),
        Ok(false) => tracing::debug!("Config already exists - using existing configuration"),
        Err(e) => tracing::error!(
            "Failed to ensure desktop config: {}. This is unexpected for desktop.",
            e
        ),
    }
}

#[cfg(mobile)]
fn adopt_app_container(app_data_dir: &Path, config_path: &Path) {
    std::env::set_var("PIPEDASH_DATA_DIR", app_data_dir.as_os_str());
    match config::follow_app_container(config_path) {
        Ok(true) => tracing::info!("Storage now follows the app container"),
        Ok(false) => {}
        Err(e) => tracing::error!("Failed to update storage location: {}", e),
    }
}

#[cfg(target_os = "android")]
fn use_app_cache_dir(app: &App) {
    let Ok(cache) = app.path().app_cache_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&cache);
    std::env::set_var("TMPDIR", &cache);
}

#[cfg_attr(not(desktop), allow(unused_variables))]
fn register_services(
    app: &App, app_data_dir: &Path, lock_settings: AppLockSettings,
) -> SetupResult {
    #[cfg(desktop)]
    {
        app.manage(host::desktop::HostEvents::default());
        app.manage(host::desktop::HostState::load(app_data_dir));
    }
    native::sync_privacy_cover(app.handle(), lock_settings);
    #[cfg(target_os = "android")]
    local_notifications::set_storage_dir(app_data_dir);
    local_notifications::refresh::register(app.handle())?;
    #[cfg(desktop)]
    {
        tauri::async_runtime::block_on(desktop::initialize(
            app.handle(),
            app_data_dir.to_path_buf(),
        ))
        .map_err(|error| error.error)?;
        system_notifications::register(app.handle())?;
        build_watch::register(app.handle())?;
    }
    Ok(())
}

fn start_mode(
    app: &App, mode: &SavedMode, setup_complete: bool, start_locked: bool,
) -> SetupResult {
    match mode {
        SavedMode::Unset => {
            tracing::info!("No run mode chosen - waiting for onboarding");
            Ok(())
        }
        SavedMode::Client { .. } => {
            remote::start(app.handle());
            Ok(())
        }
        SavedMode::Standalone => start_standalone(app, setup_complete, start_locked),
    }
}

fn start_standalone(app: &App, setup_complete: bool, start_locked: bool) -> SetupResult {
    boot_standalone_core(app, setup_complete, start_locked)?;

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        host::start_if_enabled(&handle).await;
    });
    Ok(())
}

fn boot_standalone_core(app: &App, setup_complete: bool, start_locked: bool) -> SetupResult {
    if !setup_complete {
        tracing::info!("No configuration found - starting in setup mode");
        tracing::info!("Setup wizard will be shown to complete initial configuration");
        return Ok(());
    }
    if start_locked {
        tracing::info!("App lock enabled - starting locked; the core loads after unlock");
        return Ok(());
    }

    tracing::info!("Configuration found - initializing application");
    let outcome =
        tauri::async_runtime::block_on(core_boot::boot_core(app.handle(), BootKind::Launch, false));
    #[cfg(desktop)]
    if matches!(&outcome, Err(core_boot::BootError::Config(_))) {
        desktop::initialization_failed(app.handle(), "Configuration needs attention");
    }
    core_boot::launch_outcome(outcome)?;
    Ok(())
}
