use std::{
    io::Write,
    path::{
        Path,
        PathBuf,
    },
    sync::{
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
    },
    time::{
        Duration,
        Instant,
    },
};

use futures::future::{
    AbortHandle,
    AbortRegistration,
    Abortable,
};
use pipedash_core::infrastructure::{
    config::ConfigLoader,
    http_client::parse_proxy_url,
};
use pipedash_plugin_api::http::read_prefix;
use serde::{
    Deserialize,
    Serialize,
};
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    RunEvent,
    Window,
    WindowEvent,
};
use tauri_plugin_updater::{
    RemoteRelease,
    Update,
    Updater,
    UpdaterExt,
};
use tokio::sync::{
    Mutex as AsyncMutex,
    Notify,
};

use crate::{
    app_lock::AppLockState,
    commands::ErrorResponse,
    core_boot::{
        boot_core,
        BootKind,
    },
    desktop_lifecycle::DesktopLifecycle,
    AppDataDir,
};

const FEED: &str = "https://github.com/hcavarsan/pipedash/releases/latest/download/latest.json";
const RELEASES: &str = "https://github.com/hcavarsan/pipedash/releases/";
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_FEED_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    Native,
    External,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Idle,
    Checking,
    Available,
    Downloading,
    Ready,
    Preparing,
    Installing,
    Restarting,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateFailure {
    pub stage: String,
    pub code: String,
    pub message: String,
}

impl UpdateFailure {
    fn new(stage: &str, code: &str, message: &str) -> Self {
        Self {
            stage: stage.to_owned(),
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }
}

impl From<UpdateFailure> for ErrorResponse {
    fn from(value: UpdateFailure) -> Self {
        Self {
            error: value.message,
            details: None,
            code: Some(value.code),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateNotice {
    pub version: String,
    pub outcome: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateStatus {
    pub revision: u64,
    pub current_version: String,
    pub automatic_updates: bool,
    pub mode: UpdateMode,
    pub install_source: String,
    pub reason: Option<String>,
    pub target: Option<String>,
    pub phase: UpdatePhase,
    pub available_version: Option<String>,
    pub release_url: Option<String>,
    pub last_checked_at: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub can_check: bool,
    pub can_download: bool,
    pub can_install: bool,
    pub error: Option<UpdateFailure>,
    pub notice: Option<UpdateNotice>,
}

#[derive(Serialize, Deserialize)]
struct Preferences {
    automatic_updates: bool,
}

#[derive(Serialize, Deserialize)]
struct Attempt {
    from_version: String,
    to_version: String,
}

fn attempt_notice(
    attempt: Attempt, current: &semver::Version,
) -> Result<Option<UpdateNotice>, UpdateFailure> {
    let target = semver::Version::parse(&attempt.to_version).map_err(|_| storage_error())?;
    let from = semver::Version::parse(&attempt.from_version).map_err(|_| storage_error())?;
    if target <= from {
        return Err(storage_error());
    }
    let outcome = if current == &target {
        "updated"
    } else if current == &from {
        "incomplete"
    } else if current > &target {
        return Ok(None);
    } else {
        return Err(storage_error());
    };
    Ok(Some(UpdateNotice {
        version: attempt.to_version,
        outcome: outcome.into(),
    }))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Automatic,
    Manual,
}

struct Active {
    generation: u64,
    origin: Origin,
    abort: AbortHandle,
    fallback: UpdatePhase,
}

struct Inner {
    status: UpdateStatus,
    update: Option<Update>,
    staged: bool,
    staged_size: u64,
    generation: u64,
    active: Option<Active>,
    last_attempt: Option<Instant>,
    immediate: bool,
    initialized: bool,
    storage_available: bool,
}

impl Inner {
    fn automatic_due(&mut self) -> bool {
        let due = self.immediate
            || self
                .last_attempt
                .is_none_or(|time| time.elapsed() >= CHECK_INTERVAL);
        self.immediate = false;
        due && self.status.automatic_updates && self.initialized && self.active.is_none()
    }

    fn cancel_automatic(&mut self) {
        if self
            .active
            .as_ref()
            .is_some_and(|op| op.origin == Origin::Automatic)
        {
            if let Some(op) = self.active.take() {
                op.abort.abort();
                self.restore_phase(op.fallback);
            }
            self.generation += 1;
        }
    }

    fn restore_phase(&mut self, fallback: UpdatePhase) {
        self.status.phase = if self.staged {
            if let Some(update) = &self.update {
                self.status.available_version = Some(update.version.clone());
                self.status.release_url = Some(format!("{RELEASES}tag/v{}", update.version));
            }
            self.status.downloaded_bytes = self.staged_size;
            self.status.total_bytes = Some(self.staged_size);
            UpdatePhase::Ready
        } else {
            fallback
        };
    }
}

#[derive(Clone)]
pub struct UpdaterState {
    inner: Arc<Mutex<Inner>>,
    staging: Arc<AsyncMutex<()>>,
    preferences: Arc<AsyncMutex<()>>,
    wake: Arc<Notify>,
    data: PathBuf,
    cache: Option<PathBuf>,
}

impl UpdaterState {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn publish(&self, app: &AppHandle, state: &mut Inner) -> UpdateStatus {
        let idle =
            state.active.is_none() && state.initialized && !app.state::<DesktopLifecycle>().busy();
        let native = state.status.mode == UpdateMode::Native;
        state.status.can_check = idle && state.status.mode != UpdateMode::Unsupported;
        state.status.can_download =
            idle && native && state.storage_available && state.update.is_some() && !state.staged;
        state.status.can_install = idle && native && state.storage_available && state.staged;
        state.status.revision += 1;
        let snapshot = state.status.clone();
        if let Err(error) = app.emit("desktop-update-changed", &snapshot) {
            tracing::warn!("Could not publish desktop update state: {}", error);
        }
        snapshot
    }

    fn matches(&self, generation: u64) -> bool {
        self.inner()
            .active
            .as_ref()
            .is_some_and(|op| op.generation == generation)
    }

    fn finish(&self, app: &AppHandle, generation: u64, error: Option<UpdateFailure>) {
        let mut state = self.inner();
        if state
            .active
            .as_ref()
            .is_none_or(|op| op.generation != generation)
        {
            return;
        }
        if let Some(op) = state.active.take() {
            if matches!(
                state.status.phase,
                UpdatePhase::Checking
                    | UpdatePhase::Downloading
                    | UpdatePhase::Preparing
                    | UpdatePhase::Installing
            ) {
                state.restore_phase(op.fallback);
            }
        }
        if let Some(error) = error {
            state.status.error = Some(error);
        }
        self.publish(app, &mut state);
    }
}

fn storage_error() -> UpdateFailure {
    UpdateFailure::new(
        "storage",
        "UPDATE_STORAGE_FAILED",
        "Couldn't save update data. Check free space and file permissions, then retry.",
    )
}

fn unavailable() -> ErrorResponse {
    UpdateFailure::new(
        "install",
        "UPDATE_NOT_AVAILABLE",
        "No verified update is ready for this action.",
    )
    .into()
}

fn busy() -> ErrorResponse {
    UpdateFailure::new(
        "install",
        "UPDATE_BUSY",
        "Another update operation is running.",
    )
    .into()
}

fn plugin_error(stage: &str, error: tauri_plugin_updater::Error) -> UpdateFailure {
    use tauri_plugin_updater::Error;
    match error {
        Error::SignedVersionMismatch { .. } | Error::MissingSignedVersion | Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) =>
            UpdateFailure::new(stage, "UPDATE_SIGNATURE_INVALID", "The update signature or signed version is invalid. Download the update again."),
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) =>
            UpdateFailure::new(stage, "UPDATE_TARGET_UNAVAILABLE", "This release has no update for this installation and architecture."),
        Error::PreviousAppNotRestored(path) => UpdateFailure::new("recovery", "UPDATE_RECOVERY_FAILED", &format!("Reinstall Pipedash before quitting. Your previous application is preserved at {}. Download Pipedash from {RELEASES}", path.display())),
        _ => {
            let (code, message) = match stage {
                "download" => ("UPDATE_DOWNLOAD_FAILED", "Couldn't download the update. Check your connection and retry."),
                "install" => ("UPDATE_INSTALL_FAILED", "Couldn't install the update. Check installation permissions and retry."),
                _ => ("UPDATE_CHECK_FAILED", "Couldn't check for updates. Check your connection and proxy settings, then retry."),
            };
            UpdateFailure::new(stage, code, message)
        }
    }
}

fn eligibility(app: &AppHandle) -> (UpdateMode, String, Option<String>, Option<String>) {
    use tauri::utils::config::BundleType;
    let source = tauri::utils::platform::bundle_type();
    if cfg!(debug_assertions) {
        return (
            UpdateMode::Unsupported,
            "Development build".into(),
            None,
            Some("Updates are unavailable in development builds.".into()),
        );
    }
    let arch = match std::env::consts::ARCH {
        "x86" => "i686",
        arch => arch,
    };
    let native = match source {
        #[cfg(target_os = "linux")]
        Some(BundleType::AppImage)
            if matches!(arch, "x86_64" | "aarch64")
                && app
                    .env()
                    .appimage
                    .as_ref()
                    .is_some_and(|path| Path::new(path).is_file()) =>
        {
            Some(("AppImage", format!("linux-{arch}-appimage")))
        }
        Some(BundleType::Nsis)
            if cfg!(windows) && matches!(arch, "i686" | "x86_64" | "aarch64") =>
        {
            Some(("Windows NSIS", format!("windows-{arch}-nsis")))
        }
        Some(BundleType::App)
            if cfg!(target_os = "macos") && matches!(arch, "x86_64" | "aarch64") =>
        {
            let executable = tauri::utils::platform::current_exe().ok();
            if executable
                .as_ref()
                .is_some_and(|path| mac_installable(path))
            {
                Some(("macOS application", format!("darwin-{arch}-app")))
            } else {
                return (
                    UpdateMode::External,
                    "macOS application".into(),
                    None,
                    Some("Move Pipedash to Applications, reopen it, then update.".into()),
                );
            }
        }
        _ => None,
    };
    if let Some((label, target)) = native {
        let config = app.config().plugins.0.get("updater");
        if config
            .and_then(|v| v.get("pubkey"))
            .and_then(|v| v.as_str())
            .is_none_or(str::is_empty)
            || config
                .and_then(|v| v.get("requireSignedVersion"))
                .and_then(|v| v.as_bool())
                != Some(true)
            || config
                .and_then(|v| v.get("allowDowngrades"))
                .and_then(|v| v.as_bool())
                != Some(false)
        {
            return (UpdateMode::Unsupported, label.into(), None, Some("Release signing is not configured. Install an official updater-enabled release.".into()));
        }
        return (UpdateMode::Native, label.into(), Some(target), None);
    }
    let label = match source {
        Some(BundleType::Deb) => "deb package",
        Some(BundleType::Rpm) => "RPM package",
        _ => "External installation",
    };
    (
        UpdateMode::External,
        label.into(),
        None,
        Some("Install updates using your package manager or the official release download.".into()),
    )
}

fn mac_installable(path: &Path) -> bool {
    !path.starts_with("/Volumes")
        && !path
            .components()
            .any(|part| part.as_os_str() == "AppTranslocation")
        && path.parent().is_some_and(|parent| {
            parent.file_name().is_some_and(|name| name == "MacOS")
                && parent.parent().is_some_and(|contents| {
                    contents.file_name().is_some_and(|name| name == "Contents")
                        && contents.parent().is_some_and(|bundle| {
                            bundle.extension().is_some_and(|ext| ext == "app")
                        })
                })
        })
}

fn valid_artifact(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .path()
            .starts_with("/hcavarsan/pipedash/releases/download/")
        && url.query().is_none()
        && url.fragment().is_none()
}

fn public_update_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
                    | "github-releases.githubusercontent.com"
            )
        )
}

fn update_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > 5 {
            attempt.error("Too many update redirects")
        } else if public_update_url(attempt.url()) {
            attempt.follow()
        } else {
            attempt.error("Update redirect destination refused")
        }
    })
}

async fn read_release(
    response: reqwest::Response,
) -> Result<Option<serde_json::Value>, UpdateFailure> {
    let failed = || plugin_error("check", tauri_plugin_updater::Error::ReleaseNotFound);
    let response = response.error_for_status().map_err(|_| failed())?;
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    let oversized = || {
        UpdateFailure::new(
            "check",
            "UPDATE_METADATA_TOO_LARGE",
            "The update feed exceeds the size limit. Retry after the release publisher fixes the feed.",
        )
    };
    if response
        .content_length()
        .is_some_and(|size| size > MAX_FEED_BYTES as u64)
    {
        return Err(oversized());
    }
    let bytes = read_prefix(response, MAX_FEED_BYTES + 1)
        .await
        .map_err(|_| failed())?;
    if bytes.len() > MAX_FEED_BYTES {
        return Err(oversized());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| failed())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing update directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn remove_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn clear_stage(cache: &Path) -> std::io::Result<()> {
    remove_file(&cache.join("release.json"))?;
    remove_stage_packages(cache, None)
}

fn remove_stage_packages(cache: &Path, keep: Option<&Path>) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(cache) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if keep == Some(path.as_path()) {
            continue;
        }
        if entry.file_name().to_str().is_some_and(|name| {
            name == "package"
                || name
                    .strip_prefix("package-")
                    .is_some_and(|version| semver::Version::parse(version).is_ok())
        }) {
            remove_file(&path)?;
        }
    }
    Ok(())
}

fn package_path(cache: &Path, version: &str) -> std::io::Result<PathBuf> {
    let version = semver::Version::parse(version).map_err(std::io::Error::other)?;
    Ok(cache.join(format!("package-{version}")))
}

fn read_package(cache: &Path, version: &str) -> std::io::Result<Vec<u8>> {
    std::fs::read(package_path(cache, version)?)
}

fn persist_stage(
    cache: &Path, version: &str, metadata: &serde_json::Value, bytes: &[u8],
) -> std::io::Result<()> {
    let metadata = serde_json::to_vec(metadata)?;
    let package = package_path(cache, version)?;
    atomic_write(&package, bytes)?;
    atomic_write(&cache.join("release.json"), &metadata)?;
    let _ = remove_stage_packages(cache, Some(&package));
    Ok(())
}

fn load_preferences(path: &Path) -> (bool, Option<UpdateFailure>) {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Preferences>(&bytes) {
            Ok(value) => (value.automatic_updates, None),
            Err(_) => (false, Some(UpdateFailure::new("storage", "UPDATE_STORAGE_FAILED", "Couldn't read update settings. Automatic downloads are off until you save the setting again."))),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (true, None),
        Err(_) => (false, Some(storage_error())),
    }
}

async fn proxy(app: &AppHandle) -> Result<Option<reqwest::Url>, UpdateFailure> {
    let path = app.state::<AppDataDir>().0.join("config.toml");
    let value = tauri::async_runtime::spawn_blocking(move || {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| ConfigLoader::parse_raw(&text).ok())
            .map(|config| config.general.proxy_url)
            .unwrap_or_default()
    })
    .await
    .map_err(|_| storage_error())?;
    parse_proxy_url(&value).map_err(|_| {
        UpdateFailure::new(
            "check",
            "UPDATE_CHECK_FAILED",
            "The configured update proxy is invalid. Correct it in General settings.",
        )
    })
}

async fn builder(app: &AppHandle, target: &str) -> Result<Updater, UpdateFailure> {
    let mut builder = app
        .updater_builder()
        .target(target)
        .version_comparator(|current, release| {
            release.version.pre.is_empty() && release.version > current
        })
        .timeout(Duration::from_secs(30))
        .configure_client(|client| client.https_only(true).redirect(update_redirect_policy()));
    #[cfg(windows)]
    {
        builder = builder.on_before_exit(|| {}).restart_after_install(true);
    }
    if let Some(proxy) = proxy(app).await? {
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|error| plugin_error("check", error))
}

pub fn register(app: &AppHandle) -> Result<(), String> {
    let (mode, install_source, target, reason) = eligibility(app);
    let data = app.state::<AppDataDir>().0.clone();
    let cache = app
        .path()
        .app_cache_dir()
        .ok()
        .map(|path| path.join("updates"));
    let state = UpdaterState {
        inner: Arc::new(Mutex::new(Inner {
            status: UpdateStatus {
                revision: 0,
                current_version: app.package_info().version.to_string(),
                automatic_updates: true,
                mode,
                install_source,
                reason,
                target,
                phase: UpdatePhase::Idle,
                available_version: None,
                release_url: None,
                last_checked_at: None,
                downloaded_bytes: 0,
                total_bytes: None,
                can_check: false,
                can_download: false,
                can_install: false,
                error: None,
                notice: None,
            },
            update: None,
            staged: false,
            staged_size: 0,
            generation: 0,
            active: None,
            last_attempt: None,
            immediate: false,
            initialized: mode == UpdateMode::Unsupported,
            storage_available: false,
        })),
        staging: Arc::new(AsyncMutex::new(())),
        preferences: Arc::new(AsyncMutex::new(())),
        wake: Arc::new(Notify::new()),
        data,
        cache,
    };
    app.manage(state.clone());
    if mode == UpdateMode::Unsupported {
        return Ok(());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        initialize(&handle, &state).await;
        scheduler(handle, state).await;
    });
    Ok(())
}

async fn initialize(app: &AppHandle, state: &UpdaterState) {
    let _preferences = state.preferences.lock().await;
    let _staging = state.staging.lock().await;
    let data = state.data.clone();
    let cache = state.cache.clone();
    let current = app.package_info().version.clone();
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        let preferences = load_preferences(&data.join("update-settings.json"));
        let storage = cache
            .as_ref()
            .is_some_and(|path| std::fs::create_dir_all(path).is_ok());
        let attempt = match std::fs::read(data.join("update-attempt.json")) {
            Ok(bytes) => serde_json::from_slice::<Attempt>(&bytes)
                .map(Some)
                .map_err(|_| storage_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(storage_error()),
        };
        let mut notice = None;
        let mut attempt_error = None;
        match attempt {
            Ok(Some(attempt)) => match attempt_notice(attempt, &current) {
                Ok(Some(value)) => notice = Some(value),
                Ok(None) => {
                    if remove_file(&data.join("update-attempt.json")).is_err() {
                        attempt_error = Some(storage_error());
                    }
                }
                Err(error) => attempt_error = Some(error),
            },
            Ok(None) => {}
            Err(error) => attempt_error = Some(error),
        }
        (preferences, storage, notice, attempt_error)
    })
    .await;
    match loaded {
        Ok(((automatic, error), storage, notice, attempt_error)) => {
            let mut inner = state.inner();
            inner.status.automatic_updates = automatic;
            inner.status.error = error
                .or(attempt_error)
                .or_else(|| (!storage).then(storage_error));
            inner.storage_available = storage;
            inner.status.notice = notice;
        }
        Err(_) => {
            state.inner().status.error = Some(storage_error());
            state.inner().status.automatic_updates = false;
        }
    }
    let should_restore = {
        let inner = state.inner();
        inner.status.mode == UpdateMode::Native && inner.storage_available
    };
    if should_restore {
        if let Err(error) = restore(app, state).await {
            state.inner().status.error = Some(error);
        }
    }
    let mut inner = state.inner();
    inner.initialized = true;
    state.publish(app, &mut inner);
}

async fn restore(app: &AppHandle, state: &UpdaterState) -> Result<(), UpdateFailure> {
    let target = state
        .inner()
        .status
        .target
        .clone()
        .ok_or_else(storage_error)?;
    let updater = builder(app, &target).await?;
    let cache = state.cache.clone().ok_or_else(storage_error)?;
    let restored = tauri::async_runtime::spawn_blocking(move || {
        let metadata = match std::fs::read(cache.join("release.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(storage_error()),
        };
        let result = (|| {
            let metadata = serde_json::from_slice(&metadata).map_err(|_| storage_error())?;
            let Some(mut update) = updater
                .restore_update(metadata)
                .map_err(|error| plugin_error("download", error))?
            else {
                return Ok(None);
            };
            if !valid_artifact(&update.download_url) {
                return Err(plugin_error(
                    "download",
                    tauri_plugin_updater::Error::InsecureTransportProtocol,
                ));
            }
            let bytes = read_package(&cache, &update.version).map_err(|_| storage_error())?;
            update
                .verify(&bytes)
                .map_err(|error| plugin_error("download", error))?;
            update.timeout = Some(Duration::from_secs(600));
            Ok(Some((update, bytes.len() as u64)))
        })();
        if !matches!(result, Ok(Some(_))) {
            clear_stage(&cache).map_err(|_| storage_error())?;
        }
        result
    })
    .await
    .map_err(|_| storage_error())??;
    if let Some((update, size)) = restored {
        let mut inner = state.inner();
        set_available(&mut inner, &update.version);
        inner.update = Some(update);
        inner.staged = true;
        inner.staged_size = size;
        inner.status.phase = UpdatePhase::Ready;
        inner.status.downloaded_bytes = size;
        inner.status.total_bytes = Some(size);
    }
    Ok(())
}

async fn scheduler(app: AppHandle, state: UpdaterState) {
    let start = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut timer = tokio::time::interval_at(start, CHECK_INTERVAL);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { _ = timer.tick() => {}, _ = state.wake.notified() => {} }
        let due = state.inner().automatic_due();
        if due {
            let _ = start_check(&app, Origin::Automatic);
        }
    }
}

fn set_available(inner: &mut Inner, version: &str) {
    inner.status.available_version = Some(version.to_owned());
    inner.status.release_url = Some(format!("{RELEASES}tag/v{version}"));
}

fn apply_check(
    inner: &mut Inner, update: Option<Update>, external_version: Option<String>, origin: Origin,
) -> Result<Option<Update>, UpdateFailure> {
    let automatic =
        origin == Origin::Automatic && inner.status.automatic_updates && inner.storage_available;
    if inner.staged {
        inner.restore_phase(UpdatePhase::Ready);
        if automatic {
            if let (Some(candidate), Some(staged)) = (&update, &inner.update) {
                let candidate =
                    semver::Version::parse(&candidate.version).map_err(|_| storage_error())?;
                let staged =
                    semver::Version::parse(&staged.version).map_err(|_| storage_error())?;
                if candidate > staged {
                    return Ok(update);
                }
            }
        }
        return Ok(None);
    }
    inner.status.available_version = None;
    inner.status.release_url = None;
    inner.status.downloaded_bytes = 0;
    inner.status.total_bytes = None;
    if let Some(update) = &update {
        set_available(inner, &update.version);
    } else if let Some(version) = external_version {
        set_available(inner, &version);
    }
    inner.update = update;
    inner.status.phase = if inner.status.available_version.is_some() {
        UpdatePhase::Available
    } else {
        UpdatePhase::Idle
    };
    Ok(automatic.then(|| inner.update.clone()).flatten())
}

fn claim(inner: &mut Inner, origin: Origin, phase: UpdatePhase) -> (u64, AbortRegistration) {
    inner.generation += 1;
    let generation = inner.generation;
    let (abort, registration) = AbortHandle::new_pair();
    let fallback = if inner.staged {
        UpdatePhase::Ready
    } else if inner.status.available_version.is_some() {
        UpdatePhase::Available
    } else {
        UpdatePhase::Idle
    };
    inner.active = Some(Active {
        generation,
        origin,
        abort,
        fallback,
    });
    inner.status.phase = phase;
    if inner
        .status
        .error
        .as_ref()
        .is_none_or(|error| error.stage != "storage")
    {
        inner.status.error = None;
    }
    (generation, registration)
}

fn start_check(app: &AppHandle, origin: Origin) -> Result<UpdateStatus, ErrorResponse> {
    let state = app.state::<UpdaterState>().inner().clone();
    let mut inner = state.inner();
    if inner.active.is_some() {
        return Ok(inner.status.clone());
    }
    if !inner.initialized || inner.status.mode == UpdateMode::Unsupported {
        return Err(UpdateFailure::new(
            "check",
            "NOT_SUPPORTED",
            inner
                .status
                .reason
                .as_deref()
                .unwrap_or("Updates are initializing."),
        )
        .into());
    }
    if app.state::<DesktopLifecycle>().busy() {
        return Err(busy());
    }
    if origin == Origin::Automatic && !inner.status.automatic_updates {
        return Ok(inner.status.clone());
    }
    let (generation, cancel) = claim(&mut inner, origin, UpdatePhase::Checking);
    inner.last_attempt = Some(Instant::now());
    let snapshot = state.publish(app, &mut inner);
    drop(inner);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = check(&handle, &state, generation, cancel, origin).await;
        state.finish(&handle, generation, result.err());
    });
    Ok(snapshot)
}

async fn check(
    app: &AppHandle, state: &UpdaterState, generation: u64, cancel: AbortRegistration,
    origin: Origin,
) -> Result<(), UpdateFailure> {
    let target = state.inner().status.target.clone();
    let network = async {
        let mut client = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(30))
            .redirect(update_redirect_policy());
        if let Some(proxy) = proxy(app).await? {
            client = client.proxy(reqwest::Proxy::all(proxy.as_str()).map_err(|_| {
                plugin_error("check", tauri_plugin_updater::Error::ReleaseNotFound)
            })?);
        }
        let response = client
            .build()
            .map_err(|_| plugin_error("check", tauri_plugin_updater::Error::ReleaseNotFound))?
            .get(FEED)
            .send()
            .await
            .map_err(|_| plugin_error("check", tauri_plugin_updater::Error::ReleaseNotFound))?;
        let Some(metadata) = read_release(response).await? else {
            return Ok((None, None));
        };
        if let Some(target) = target {
            let update = builder(app, &target)
                .await?
                .restore_update(metadata)
                .map_err(|error| plugin_error("check", error))?;
            Ok((update, None))
        } else {
            let release: RemoteRelease = serde_json::from_value(metadata)
                .map_err(|_| plugin_error("check", tauri_plugin_updater::Error::ReleaseNotFound))?;
            let version = (release.version.pre.is_empty()
                && release.version > app.package_info().version)
                .then(|| release.version.to_string());
            Ok((None, version))
        }
    };
    let (mut update, external_version) = match Abortable::new(network, cancel).await {
        Ok(result) => result?,
        Err(_) => return Ok(()),
    };
    if !state.matches(generation) {
        return Ok(());
    }
    if let Some(update) = &mut update {
        if !valid_artifact(&update.download_url) {
            return Err(plugin_error(
                "check",
                tauri_plugin_updater::Error::InsecureTransportProtocol,
            ));
        }
        update.timeout = Some(Duration::from_secs(600));
    }
    let _staging = state.staging.lock().await;
    if !state.matches(generation) {
        return Ok(());
    }
    let automatic_download;
    {
        let mut inner = state.inner();
        if inner
            .active
            .as_ref()
            .is_none_or(|op| op.generation != generation)
        {
            return Ok(());
        }
        inner.status.last_checked_at = Some(chrono::Utc::now().to_rfc3339());
        automatic_download = apply_check(&mut inner, update, external_version, origin)?;
        state.publish(app, &mut inner);
    }
    drop(_staging);
    if let Some(update) = automatic_download {
        let cancel = {
            let mut inner = state.inner();
            if inner
                .active
                .as_ref()
                .is_none_or(|op| op.generation != generation)
            {
                return Ok(());
            }
            let (abort, cancel) = AbortHandle::new_pair();
            let fallback = inner.status.phase;
            if let Some(op) = &mut inner.active {
                op.abort = abort;
                op.fallback = fallback;
            }
            set_available(&mut inner, &update.version);
            inner.status.phase = UpdatePhase::Downloading;
            inner.status.downloaded_bytes = 0;
            inner.status.total_bytes = None;
            state.publish(app, &mut inner);
            cancel
        };
        download(app, state, generation, cancel, update).await?;
    }
    Ok(())
}

async fn download(
    app: &AppHandle, state: &UpdaterState, generation: u64, cancel: AbortRegistration,
    update: Update,
) -> Result<(), UpdateFailure> {
    if !valid_artifact(&update.download_url) {
        return Err(plugin_error(
            "download",
            tauri_plugin_updater::Error::InsecureTransportProtocol,
        ));
    }
    let worker_state = state.clone();
    let handle = app.clone();
    let (update, bytes) = tauri::async_runtime::spawn_blocking(move || {
        let mut last_progress = Instant::now();
        let bytes = tauri::async_runtime::block_on(Abortable::new(
            update.download(
                move |size, total| {
                    let mut inner = worker_state.inner();
                    if inner
                        .active
                        .as_ref()
                        .is_none_or(|op| op.generation != generation)
                    {
                        return;
                    }
                    inner.status.downloaded_bytes += size as u64;
                    inner.status.total_bytes = total;
                    if last_progress.elapsed() >= Duration::from_millis(250) {
                        worker_state.publish(&handle, &mut inner);
                        last_progress = Instant::now();
                    }
                },
                || {},
            ),
            cancel,
        ));
        (update, bytes)
    })
    .await
    .map_err(|_| plugin_error("download", tauri_plugin_updater::Error::ReleaseNotFound))?;
    let bytes = match bytes {
        Ok(result) => result.map_err(|error| plugin_error("download", error))?,
        Err(_) => return Ok(()),
    };
    let _staging = state.staging.lock().await;
    if !state.matches(generation) {
        return Ok(());
    }
    let cache = state.cache.clone().ok_or_else(storage_error)?;
    let size = bytes.len() as u64;
    let update = tauri::async_runtime::spawn_blocking(move || {
        persist_stage(&cache, &update.version, &update.raw_json, &bytes)?;
        Ok::<_, std::io::Error>(update)
    })
    .await
    .map_err(|_| storage_error())?
    .map_err(|_| storage_error())?;
    let mut inner = state.inner();
    if inner
        .active
        .as_ref()
        .is_some_and(|op| op.generation == generation)
    {
        set_available(&mut inner, &update.version);
        inner.update = Some(update);
        inner.staged = true;
        inner.staged_size = size;
        inner.status.phase = UpdatePhase::Ready;
        inner.status.downloaded_bytes = size;
        inner.status.total_bytes = Some(size);
        state.publish(app, &mut inner);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_update_status(app: AppHandle) -> Result<UpdateStatus, ErrorResponse> {
    Ok(app.state::<UpdaterState>().inner().inner().status.clone())
}

#[tauri::command]
pub async fn set_automatic_updates(
    app: AppHandle, enabled: bool,
) -> Result<UpdateStatus, ErrorResponse> {
    let state = app.state::<UpdaterState>().inner().clone();
    let _preferences = state.preferences.lock().await;
    if state.inner().status.mode == UpdateMode::Unsupported {
        return Err(UpdateFailure::new(
            "storage",
            "NOT_SUPPORTED",
            "Update preferences are unavailable for this installation.",
        )
        .into());
    }
    if app.state::<DesktopLifecycle>().busy() {
        return Err(busy());
    }
    let path = state.data.join("update-settings.json");
    let bytes = serde_json::to_vec(&Preferences {
        automatic_updates: enabled,
    })
    .map_err(|_| ErrorResponse::from(storage_error()))?;
    tauri::async_runtime::spawn_blocking(move || atomic_write(&path, &bytes))
        .await
        .map_err(|_| ErrorResponse::from(storage_error()))?
        .map_err(|_| ErrorResponse::from(storage_error()))?;
    let _staging = state.staging.lock().await;
    let mut inner = state.inner();
    inner.status.automatic_updates = enabled;
    if inner
        .status
        .error
        .as_ref()
        .is_some_and(|error| error.stage == "storage")
    {
        inner.status.error = None;
    }
    if !enabled {
        inner.cancel_automatic();
    }
    if enabled {
        inner.immediate = true;
        state.wake.notify_one();
    }
    Ok(state.publish(&app, &mut inner))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateStatus, ErrorResponse> {
    start_check(&app, Origin::Manual)
}

#[tauri::command]
pub async fn download_update(app: AppHandle) -> Result<UpdateStatus, ErrorResponse> {
    let state = app.state::<UpdaterState>().inner().clone();
    let mut inner = state.inner();
    if inner.active.is_some() {
        return Ok(inner.status.clone());
    }
    if app.state::<DesktopLifecycle>().busy() {
        return Err(busy());
    }
    if !inner.status.can_download {
        return Err(unavailable());
    }
    let update = inner.update.clone().ok_or_else(unavailable)?;
    let (generation, cancel) = claim(&mut inner, Origin::Manual, UpdatePhase::Downloading);
    inner.status.downloaded_bytes = 0;
    inner.status.total_bytes = None;
    let snapshot = state.publish(&app, &mut inner);
    drop(inner);
    tauri::async_runtime::spawn(async move {
        let result = download(&app, &state, generation, cancel, update).await;
        state.finish(&app, generation, result.err());
    });
    Ok(snapshot)
}

#[tauri::command]
pub async fn install_update(app: AppHandle, version: String) -> Result<(), ErrorResponse> {
    let state = app.state::<UpdaterState>().inner().clone();
    let mut inner = state.inner();
    if inner.active.is_some() || app.state::<DesktopLifecycle>().busy() {
        return Err(busy());
    }
    if !inner.status.can_install {
        return Err(unavailable());
    }
    if inner.status.available_version.as_deref() != Some(&version) {
        return Err(UpdateFailure::new(
            "install",
            "UPDATE_CHANGED",
            "A different update is ready. Review it before installing.",
        )
        .into());
    }
    let (generation, _) = claim(&mut inner, Origin::Manual, UpdatePhase::Preparing);
    state.publish(&app, &mut inner);
    drop(inner);
    tauri::async_runtime::spawn(async move {
        let result = install(&app, &state, generation, &version).await;
        state.finish(&app, generation, result.err());
    });
    Ok(())
}

async fn install(
    app: &AppHandle, state: &UpdaterState, generation: u64, version: &str,
) -> Result<(), UpdateFailure> {
    let lifecycle = app.state::<DesktopLifecycle>();
    let exclusive = lifecycle.acquire_exclusive().await.map_err(|_| {
        UpdateFailure::new(
            "install",
            "UPDATE_BUSY",
            "Another restart or update is running.",
        )
    })?;
    let _preferences = state.preferences.lock().await;
    let _staging = state.staging.lock().await;
    if !state.matches(generation) {
        return Ok(());
    }
    if eligibility(app).0 != UpdateMode::Native {
        return Err(plugin_error(
            "install",
            tauri_plugin_updater::Error::UnsupportedOs,
        ));
    }
    let update = {
        let inner = state.inner();
        if !inner.staged || inner.status.available_version.as_deref() != Some(version) {
            return Err(UpdateFailure::new(
                "install",
                "UPDATE_CHANGED",
                "A different update is ready. Review it before installing.",
            ));
        }
        inner
            .update
            .clone()
            .ok_or_else(|| plugin_error("install", tauri_plugin_updater::Error::ReleaseNotFound))?
    };
    let cache = state.cache.clone().ok_or_else(storage_error)?;
    let verified = tauri::async_runtime::spawn_blocking(move || {
        let result = read_package(&cache, &update.version)
            .map_err(|_| storage_error())
            .and_then(|bytes| {
                update
                    .verify(&bytes)
                    .map(|()| bytes)
                    .map_err(|error| plugin_error("install", error))
            });
        match result {
            Ok(bytes) => Ok((update, bytes)),
            Err(error) => {
                clear_stage(&cache).map_err(|_| storage_error())?;
                Err(error)
            }
        }
    })
    .await
    .map_err(|_| storage_error())?;
    let (update, bytes) = match verified {
        Ok(value) => value,
        Err(error) => {
            let mut inner = state.inner();
            inner.staged = false;
            inner.staged_size = 0;
            inner.status.phase = UpdatePhase::Available;
            if let Some(op) = &mut inner.active {
                op.fallback = UpdatePhase::Available;
            }
            return Err(error);
        }
    };
    let lock = app.state::<AppLockState>();
    let transition = lock.transition().await;
    let had_core = lifecycle.drain_core(app).await.map_err(|_| {
        UpdateFailure::new(
            "install",
            "UPDATE_INSTALL_FAILED",
            "Couldn't finish application startup. Reopen Pipedash before updating.",
        )
    })?;
    let attempt = Attempt {
        from_version: app.package_info().version.to_string(),
        to_version: version.to_owned(),
    };
    let path = state.data.join("update-attempt.json");
    let result = async {
        tauri::async_runtime::spawn_blocking(move || {
            let bytes = serde_json::to_vec(&attempt).map_err(|_| storage_error())?;
            atomic_write(&path, &bytes).map_err(|_| storage_error())
        })
        .await
        .map_err(|_| storage_error())??;
        {
            let mut inner = state.inner();
            inner.status.phase = UpdatePhase::Installing;
            state.publish(app, &mut inner);
        }
        tauri::async_runtime::spawn_blocking(move || update.install(bytes))
            .await
            .map_err(|_| {
                UpdateFailure::new(
                    "install",
                    "UPDATE_INSTALL_FAILED",
                    "The installer stopped unexpectedly. Reopen Pipedash before retrying.",
                )
            })?
            .map_err(|error| plugin_error("install", error))
    }
    .await;
    drop(transition);
    match result {
        Ok(()) => {
            let mut inner = state.inner();
            inner.status.phase = UpdatePhase::Restarting;
            state.publish(app, &mut inner);
            drop(inner);
            app.manage(exclusive);
            app.request_restart();
            Ok(())
        }
        Err(error) => {
            if had_core && boot_core(app, BootKind::Resume, false).await.is_err() {
                return Err(UpdateFailure::new(
                    "recovery",
                    "UPDATE_RECOVERY_FAILED",
                    "Couldn't restart application services. Reopen Pipedash before continuing.",
                ));
            }
            if error.code == "UPDATE_RECOVERY_FAILED" {
                let mut inner = state.inner();
                inner.staged = false;
                inner.status.phase = UpdatePhase::Available;
            }
            drop(exclusive);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn acknowledge_update_notice(
    app: AppHandle, version: String,
) -> Result<UpdateStatus, ErrorResponse> {
    let state = app.state::<UpdaterState>().inner().clone();
    let _staging = state.staging.lock().await;
    if state
        .inner()
        .status
        .notice
        .as_ref()
        .is_none_or(|notice| notice.version != version)
    {
        return Ok(state.inner().status.clone());
    }
    let path = state.data.join("update-attempt.json");
    tauri::async_runtime::spawn_blocking(move || remove_file(&path))
        .await
        .map_err(|_| ErrorResponse::from(storage_error()))?
        .map_err(|_| ErrorResponse::from(storage_error()))?;
    let mut inner = state.inner();
    inner.status.notice = None;
    Ok(state.publish(&app, &mut inner))
}

pub async fn reset(app: &AppHandle) -> Result<(), ErrorResponse> {
    let state = app.state::<UpdaterState>().inner().clone();
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let _preferences = state.preferences.lock().await;
    let _staging = state.staging.lock().await;
    {
        let mut inner = state.inner();
        if let Some(op) = inner.active.take() {
            op.abort.abort();
        }
        inner.generation += 1;
    }
    let data = state.data.clone();
    let cache = state.cache.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        remove_file(&data.join("update-settings.json"))?;
        remove_file(&data.join("update-attempt.json"))?;
        if let Some(cache) = cache {
            clear_stage(&cache)?;
        }
        Ok::<_, std::io::Error>(())
    })
    .await
    .map_err(|_| ErrorResponse::from(storage_error()))?
    .map_err(|_| ErrorResponse::from(storage_error()));
    let mut inner = state.inner();
    inner.status.automatic_updates = true;
    inner.status.available_version = None;
    inner.status.release_url = None;
    inner.status.last_checked_at = None;
    inner.status.downloaded_bytes = 0;
    inner.status.total_bytes = None;
    inner.status.notice = None;
    inner.status.phase = UpdatePhase::Idle;
    inner.update = None;
    inner.staged = false;
    inner.staged_size = 0;
    inner.status.error = result.as_ref().err().map(|_| storage_error());
    inner.immediate = true;
    state.publish(app, &mut inner);
    state.wake.notify_one();
    result
}

fn blocks_exit(app: &AppHandle) -> bool {
    app.state::<DesktopLifecycle>().busy()
        || app.try_state::<UpdaterState>().is_some_and(|state| {
            matches!(
                state.inner().inner().status.phase,
                UpdatePhase::Preparing | UpdatePhase::Installing | UpdatePhase::Restarting
            )
        })
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    match event {
        WindowEvent::CloseRequested { api, .. } if blocks_exit(app) => api.prevent_close(),
        WindowEvent::Focused(true) => {
            if let Some(state) = app.try_state::<UpdaterState>() {
                state.wake.notify_one();
            }
        }
        _ => {}
    }
}

pub fn on_run_event(app: &AppHandle, event: &RunEvent) {
    if let RunEvent::ExitRequested { api, code, .. } = event {
        if *code != Some(tauri::RESTART_EXIT_CODE) && blocks_exit(app) {
            api.prevent_exit();
        }
    }
}

#[cfg(test)]
mod tests {
    use base64::{
        engine::general_purpose::STANDARD,
        Engine,
    };

    use super::*;

    const PUBLIC_KEY: &str = "untrusted comment: minisign public key: A49EF4746FA6E6D7\nRWTX5qZvdPSepDGWg2bz8n6ArQnBMwgQK1geVosorcbLzcunD+KJ+cYe";
    const SIGNATURE: &str = "untrusted comment: signature from tauri secret key\nRUTX5qZvdPSepPF2zsjvP/L/TQtBLRnWRvaq+4omK1UEugtQp/USrGiI1v762sDy3id3aor7bRC9ETyeot6lMHHrWoZ5hjUM1Qc=\ntrusted comment: timestamp:1791649051\tfile:payload\tversion:2.0.0\nDDq2CLOEl5cQmC2eJRHlnZPVViOZsu1CjpPKQoei4A/XBfkDqGO0B+QMrcrHgEQ5qV3mne75tdFWhJpqUXh9AA==";

    fn test_client() -> reqwest::ClientBuilder {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
    }

    fn read_request(socket: &mut std::net::TcpStream) {
        use std::io::{
            BufRead,
            BufReader,
        };

        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(socket);
        let mut line = String::new();
        while line != "\r\n" {
            line.clear();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        }
    }

    fn verifier() -> Updater {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert("updater".into(), serde_json::json!({
            "pubkey": STANDARD.encode(PUBLIC_KEY), "requireSignedVersion": true,
            "allowDowngrades": false, "endpoints": [FEED], "windows": { "installMode": "passive" }
        }));
        let app = tauri::test::mock_builder()
            .plugin(
                tauri_plugin_updater::Builder::new()
                    .pubkey(STANDARD.encode(PUBLIC_KEY))
                    .build(),
            )
            .build(context)
            .unwrap();
        app.updater_builder()
            .target("test-target")
            .version_comparator(|current, release| {
                release.version.pre.is_empty() && release.version > current
            })
            .endpoints(vec![FEED.parse().unwrap()])
            .unwrap()
            .build()
            .unwrap()
    }

    fn manifest(version: &str) -> serde_json::Value {
        serde_json::json!({ "version": version, "platforms": { "test-target": {
            "url": "https://github.com/hcavarsan/pipedash/releases/download/v2.0.0/package",
            "signature": STANDARD.encode(SIGNATURE)
        }}})
    }

    #[test]
    fn cached_update_requires_real_signature_and_binds_the_announced_version() {
        let updater = verifier();
        let update = updater.restore_update(manifest("2.0.0")).unwrap().unwrap();
        update.verify(b"test").unwrap();
        assert!(matches!(
            update.verify(b"tampered"),
            Err(tauri_plugin_updater::Error::Minisign(_))
        ));
        let tampered = updater.restore_update(manifest("9.9.9")).unwrap().unwrap();
        assert!(matches!(
            tampered.verify(b"test"),
            Err(tauri_plugin_updater::Error::SignedVersionMismatch { .. })
        ));
    }

    #[test]
    fn stable_release_selection_rejects_nonupdates_and_missing_target_is_an_error() {
        let updater = verifier();
        for version in ["0.1.0", "0.0.1", "2.0.0-rc.1"] {
            assert!(updater.restore_update(manifest(version)).unwrap().is_none());
        }
        assert!(matches!(
            updater.restore_update(serde_json::json!({ "version": "2.0.0", "platforms": {} })),
            Err(tauri_plugin_updater::Error::TargetNotFound(_))
        ));
    }

    #[test]
    fn only_matching_postrestart_version_reports_success() {
        let marker = || Attempt {
            from_version: "0.8.0".into(),
            to_version: "0.8.1".into(),
        };
        assert_eq!(
            attempt_notice(marker(), &"0.8.0".parse().unwrap())
                .unwrap()
                .unwrap()
                .outcome,
            "incomplete"
        );
        assert_eq!(
            attempt_notice(marker(), &"0.8.1".parse().unwrap())
                .unwrap()
                .unwrap()
                .outcome,
            "updated"
        );
        assert!(attempt_notice(marker(), &"0.8.2".parse().unwrap())
            .unwrap()
            .is_none());
        assert!(attempt_notice(marker(), &"0.7.0".parse().unwrap()).is_err());
    }

    #[test]
    fn artifact_urls_reject_other_origins_and_credentials() {
        for url in [
            "http://github.com/hcavarsan/pipedash/releases/download/v1/a",
            "https://github.com/other/pipedash/releases/download/v1/a",
            "https://token@github.com/hcavarsan/pipedash/releases/download/v1/a",
            "https://github.com:444/hcavarsan/pipedash/releases/download/v1/a",
            "https://evil.test/hcavarsan/pipedash/releases/download/v1/a",
            "https://github.com/hcavarsan/pipedash/releases/download/v1/a?token=secret",
        ] {
            assert!(!valid_artifact(&url.parse().unwrap()), "{url}");
        }
        assert!(valid_artifact(
            &"https://github.com/hcavarsan/pipedash/releases/download/v0.8.1/a.AppImage"
                .parse()
                .unwrap()
        ));
    }

    #[test]
    fn invalid_preferences_disable_automatic_downloads_and_atomic_save_recovers() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("update-settings.json");
        assert!(load_preferences(&path).0);
        std::fs::write(&path, b"truncated").unwrap();
        let (enabled, error) = load_preferences(&path);
        assert!(!enabled);
        assert_eq!(error.unwrap().code, "UPDATE_STORAGE_FAILED");
        atomic_write(&path, br#"{"automatic_updates":false}"#).unwrap();
        let (enabled, error) = load_preferences(&path);
        assert!(!enabled);
        assert!(error.is_none());
        atomic_write(&path, br#"{"automatic_updates":true}"#).unwrap();
        assert!(load_preferences(&path).0);
    }

    fn manager() -> UpdaterState {
        UpdaterState {
            inner: Arc::new(Mutex::new(Inner {
                status: UpdateStatus {
                    revision: 0,
                    current_version: "0.8.0".into(),
                    automatic_updates: true,
                    mode: UpdateMode::Native,
                    install_source: "AppImage".into(),
                    reason: None,
                    target: Some("linux-x86_64-appimage".into()),
                    phase: UpdatePhase::Idle,
                    available_version: None,
                    release_url: None,
                    last_checked_at: None,
                    downloaded_bytes: 0,
                    total_bytes: None,
                    can_check: true,
                    can_download: false,
                    can_install: false,
                    error: None,
                    notice: None,
                },
                update: None,
                staged: false,
                staged_size: 0,
                generation: 0,
                active: None,
                last_attempt: None,
                immediate: false,
                initialized: true,
                storage_available: true,
            })),
            staging: Arc::new(AsyncMutex::new(())),
            preferences: Arc::new(AsyncMutex::new(())),
            wake: Arc::new(Notify::new()),
            data: PathBuf::new(),
            cache: None,
        }
    }

    #[test]
    fn disabled_preferences_block_scheduling_but_not_manual_work() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("update-settings.json");
        atomic_write(&path, br#"{"automatic_updates":false}"#).unwrap();
        let state = manager();
        let mut inner = state.inner();
        inner.status.automatic_updates = load_preferences(&path).0;
        inner.immediate = true;
        assert!(!inner.automatic_due());
        let (generation, _cancel) = claim(&mut inner, Origin::Manual, UpdatePhase::Checking);
        assert_eq!(inner.active.as_ref().unwrap().generation, generation);
        inner.cancel_automatic();
        assert_eq!(inner.status.phase, UpdatePhase::Checking);
        assert!(!inner.automatic_due());
    }

    #[tokio::test]
    async fn cancelling_automatic_work_invalidates_its_generation_and_preserves_ready_packages() {
        let state = manager();
        let (generation, cancel) = claim(
            &mut state.inner(),
            Origin::Automatic,
            UpdatePhase::Downloading,
        );
        assert!(state.matches(generation));
        {
            let mut inner = state.inner();
            inner.staged = true;
            inner.cancel_automatic();
            assert_eq!(inner.status.phase, UpdatePhase::Ready);
        }
        assert!(!state.matches(generation));
        assert!(Abortable::new(futures::future::pending::<()>(), cancel)
            .await
            .is_err());
    }

    #[test]
    fn successful_empty_checks_clear_native_and_external_offers() {
        let updater = verifier();
        for mode in [UpdateMode::Native, UpdateMode::External] {
            let state = manager();
            let mut inner = state.inner();
            inner.status.mode = mode;
            inner.update = updater.restore_update(manifest("2.0.0")).unwrap();
            set_available(&mut inner, "2.0.0");
            inner.status.downloaded_bytes = 17;
            inner.status.total_bytes = Some(20);
            apply_check(&mut inner, None, None, Origin::Manual).unwrap();
            assert!(inner.update.is_none());
            assert!(inner.status.available_version.is_none());
            assert!(inner.status.release_url.is_none());
            assert_eq!(inner.status.phase, UpdatePhase::Idle);
            assert_eq!(inner.status.downloaded_bytes, 0);
            assert_eq!(inner.status.total_bytes, None);
        }
    }

    #[test]
    fn manual_checks_keep_the_verified_release_and_its_install_identity() {
        let updater = verifier();
        for candidate in [None, Some("2.0.0"), Some("3.0.0")] {
            let state = manager();
            let mut inner = state.inner();
            inner.update = updater.restore_update(manifest("2.0.0")).unwrap();
            inner.staged = true;
            set_available(&mut inner, "2.0.0");
            let candidate =
                candidate.and_then(|version| updater.restore_update(manifest(version)).unwrap());
            assert!(apply_check(&mut inner, candidate, None, Origin::Manual)
                .unwrap()
                .is_none());
            assert_eq!(inner.status.available_version.as_deref(), Some("2.0.0"));
            assert_eq!(inner.update.as_ref().unwrap().version, "2.0.0");
            assert_eq!(inner.status.phase, UpdatePhase::Ready);
            assert!(inner.staged);
        }
    }

    #[test]
    fn replacement_candidates_do_not_change_the_staged_install_identity() {
        let updater = verifier();
        let state = manager();
        let mut inner = state.inner();
        inner.update = updater.restore_update(manifest("2.0.0")).unwrap();
        inner.staged = true;
        inner.staged_size = 4;
        set_available(&mut inner, "2.0.0");
        let candidate = updater.restore_update(manifest("3.0.0")).unwrap();
        let replacement = apply_check(&mut inner, candidate, None, Origin::Automatic)
            .unwrap()
            .unwrap();
        assert_eq!(replacement.version, "3.0.0");
        assert_eq!(inner.update.as_ref().unwrap().version, "2.0.0");
        assert_eq!(inner.status.available_version.as_deref(), Some("2.0.0"));
        claim(&mut inner, Origin::Automatic, UpdatePhase::Downloading);
        set_available(&mut inner, &replacement.version);
        inner.status.downloaded_bytes = 19;
        inner.status.total_bytes = Some(20);
        inner.cancel_automatic();
        assert_eq!(inner.status.phase, UpdatePhase::Ready);
        assert_eq!(inner.update.as_ref().unwrap().version, "2.0.0");
        assert_eq!(inner.status.available_version.as_deref(), Some("2.0.0"));
        assert_eq!(inner.status.downloaded_bytes, 4);
        assert_eq!(inner.status.total_bytes, Some(4));
    }

    #[test]
    fn failed_replacement_restores_the_staged_version_and_progress() {
        let updater = verifier();
        let state = manager();
        let mut inner = state.inner();
        inner.update = updater.restore_update(manifest("2.0.0")).unwrap();
        inner.staged = true;
        inner.staged_size = 4;
        set_available(&mut inner, "2.0.0");
        claim(&mut inner, Origin::Automatic, UpdatePhase::Downloading);
        set_available(&mut inner, "3.0.0");
        inner.status.downloaded_bytes = 19;
        inner.status.total_bytes = Some(20);
        let failed = updater.restore_update(manifest("3.0.0")).unwrap().unwrap();
        assert!(failed.verify(b"test").is_err());
        let active = inner.active.take().unwrap();
        inner.restore_phase(active.fallback);
        assert_eq!(inner.status.phase, UpdatePhase::Ready);
        assert_eq!(inner.status.available_version.as_deref(), Some("2.0.0"));
        assert_eq!(inner.status.downloaded_bytes, 4);
        assert_eq!(inner.status.total_bytes, Some(4));
        inner.update.as_ref().unwrap().verify(b"test").unwrap();
    }

    #[test]
    fn failed_stage_replacement_preserves_the_previous_package_and_metadata() {
        let root = tempfile::tempdir().unwrap();
        persist_stage(root.path(), "2.0.0", &manifest("2.0.0"), b"test").unwrap();
        let old_metadata = std::fs::read(root.path().join("release.json")).unwrap();
        std::fs::create_dir(root.path().join("package-3.0.0")).unwrap();
        assert!(persist_stage(root.path(), "3.0.0", &manifest("3.0.0"), b"new").is_err());
        assert_eq!(
            std::fs::read(root.path().join("release.json")).unwrap(),
            old_metadata
        );
        assert_eq!(read_package(root.path(), "2.0.0").unwrap(), b"test");
        verifier()
            .restore_update(serde_json::from_slice(&old_metadata).unwrap())
            .unwrap()
            .unwrap()
            .verify(&read_package(root.path(), "2.0.0").unwrap())
            .unwrap();
    }

    #[test]
    fn update_redirects_require_public_https_destinations() {
        for url in [
            "http://github.com/file",
            "https://localhost/file",
            "https://localhost./file",
            "https://printer.local/file",
            "https://127.0.0.1/file",
            "https://10.0.0.1/file",
            "https://169.254.169.254/file",
            "https://[::1]/file",
            "https://[::ffff:127.0.0.1]/file",
            "https://[fc00::1]/file",
            "https://user:secret@github.com/file",
            "https://8.8.8.8/file",
            "https://example.com/file",
        ] {
            assert!(!public_update_url(&url.parse().unwrap()), "{url}");
        }
        for url in [
            "https://github.com/file",
            "https://release-assets.githubusercontent.com/file?signature=abc",
            "https://objects.githubusercontent.com/file",
        ] {
            assert!(public_update_url(&url.parse().unwrap()), "{url}");
        }
    }

    #[tokio::test]
    async fn metadata_rejects_oversized_fixed_and_chunked_bodies() {
        use std::net::TcpListener;

        for chunked in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                read_request(&mut socket);
                let body = " ".repeat(MAX_FEED_BYTES + 1);
                let response = if chunked {
                    format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n", body.len())
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let _ = socket.write_all(response.as_bytes());
            });
            let response = test_client()
                .build()
                .unwrap()
                .get(format!("http://{address}/"))
                .send()
                .await
                .unwrap();
            let error = read_release(response).await.err().unwrap();
            assert_eq!(error.code, "UPDATE_METADATA_TOO_LARGE");
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn redirect_policy_refuses_private_destinations_before_connecting() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            read_request(&mut socket);
            socket.write_all(
                b"HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            ).unwrap();
        });
        let error = test_client()
            .redirect(update_redirect_policy())
            .build()
            .unwrap()
            .get(format!("http://{address}/"))
            .send()
            .await
            .unwrap_err();
        assert!(error.is_redirect());
        server.join().unwrap();
    }

    #[test]
    fn successful_stage_replacement_commits_metadata_and_removes_the_old_package() {
        let root = tempfile::tempdir().unwrap();
        persist_stage(root.path(), "2.0.0", &manifest("2.0.0"), b"test").unwrap();
        persist_stage(root.path(), "3.0.0", &manifest("3.0.0"), b"replacement").unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.path().join("release.json")).unwrap())
                .unwrap();
        assert_eq!(metadata["version"], "3.0.0");
        assert_eq!(read_package(root.path(), "3.0.0").unwrap(), b"replacement");
        assert!(!root.path().join("package-2.0.0").exists());
        clear_stage(root.path()).unwrap();
        assert!(!root.path().join("release.json").exists());
        assert!(!root.path().join("package-3.0.0").exists());
    }

    #[tokio::test]
    async fn bounded_feed_accepts_no_content_and_metadata_at_the_limit() {
        use std::net::TcpListener;

        let metadata = manifest("2.0.0");
        let json = serde_json::to_string(&metadata).unwrap();
        let padded = format!("{json}{}", " ".repeat(MAX_FEED_BYTES - json.len()));
        for (status, body, expected) in [
            ("204 No Content", String::new(), None),
            ("200 OK", padded, Some(metadata)),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                read_request(&mut socket);
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            });
            let response = test_client()
                .build()
                .unwrap()
                .get(format!("http://{address}/"))
                .send()
                .await
                .unwrap();
            assert_eq!(read_release(response).await.unwrap(), expected);
            server.join().unwrap();
        }
    }

    #[test]
    fn mac_installations_on_images_or_translocated_paths_are_external() {
        assert!(mac_installable(Path::new(
            "/Applications/Pipedash.app/Contents/MacOS/pipedash"
        )));
        assert!(!mac_installable(Path::new(
            "/Volumes/Pipedash/Pipedash.app/Contents/MacOS/pipedash"
        )));
        assert!(!mac_installable(Path::new(
            "/private/var/AppTranslocation/abc/Pipedash.app/Contents/MacOS/pipedash"
        )));
        assert!(!mac_installable(Path::new("/usr/local/bin/pipedash")));
    }
}
