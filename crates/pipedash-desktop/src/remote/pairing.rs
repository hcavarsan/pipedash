use pipedash_core::domain::pairing::{
    normalize_pairing_code,
    parse_pairing_link,
};
use pipedash_core::domain::ServerUrl;
use tauri::{
    AppHandle,
    Manager,
};

use super::alerts;
use super::connection::RemoteConnection;
use super::model::{
    ConnectionInfo,
    RemoteError,
};
use super::session::connect::{
    open,
    revoke,
};
use super::session::protocol::ClientHello;
use super::store::{
    self,
    SavedConnection,
    SavedMode,
};
use crate::app_lock::AppLockState;
use crate::core_boot::BootKind;
use crate::{
    AppDataDir,
    MaybeCoreContext,
};

const DEVICE_NAME_MAX_CHARS: usize = 64;

pub(crate) fn check_transport(
    url: &ServerUrl, allow_private_http: bool,
) -> Result<(), RemoteError> {
    if url.scheme == "https" {
        return Ok(());
    }
    if !url.is_private() {
        return Err(RemoteError::PublicHttp);
    }
    if !allow_private_http {
        return Err(RemoteError::PrivateHttpDisabled);
    }

    Ok(())
}

fn platform_label(platform: &str) -> &'static str {
    match platform {
        "macos" => "Mac",
        "windows" => "Windows PC",
        "ios" => "iPhone or iPad",
        "android" => "Android device",
        _ => "Linux PC",
    }
}

pub(super) fn name_for(hostname: &str, platform: &str) -> String {
    let hostname = hostname.trim();
    let name: String = hostname
        .strip_suffix(".local")
        .unwrap_or(hostname)
        .chars()
        .take(DEVICE_NAME_MAX_CHARS)
        .collect();
    if name.is_empty() || name.eq_ignore_ascii_case("localhost") {
        platform_label(platform).to_owned()
    } else {
        name
    }
}

pub fn device_name() -> String {
    name_for(&tauri_plugin_os::hostname(), tauri_plugin_os::platform())
}

pub(crate) fn parse_pairing(
    address: &str, code: Option<&str>,
) -> Result<(ServerUrl, String), RemoteError> {
    let link = parse_pairing_link(address);
    let (address, code) = match &link {
        Some((url, code)) => (url.as_str(), code.as_str()),
        None => (
            address,
            code.map(str::trim)
                .filter(|code| !code.is_empty())
                .ok_or(RemoteError::CodeRequired)?,
        ),
    };
    let url = ServerUrl::parse(address).map_err(RemoteError::InvalidUrl)?;
    let code = normalize_pairing_code(code).ok_or(RemoteError::InvalidCode)?;

    Ok((url, code))
}

async fn stop_local_core(app: &AppHandle) {
    let lock = app.state::<AppLockState>();
    let _transition = lock.transition().await;

    crate::host::stop(app).await;
    #[cfg(desktop)]
    {
        crate::desktop::projection::detach(app);
        crate::desktop::flush_menu(app).await;
    }
    crate::build_watch::interrupt(app);
    if let Err(error) = crate::local_notifications::settings::reset(app).await {
        tracing::warn!(?error, "Couldn't clear the local notifications");
    }
    let core = app.state::<MaybeCoreContext>().0.write().await.take();
    crate::core_boot::background::cancel_init(app).await;
    if let Some(core) = core {
        core.shutdown().await;
    }
}

pub(super) async fn run_here(
    app: &AppHandle, remote: &RemoteConnection,
) -> Result<ConnectionInfo, RemoteError> {
    match remote.saved().mode {
        SavedMode::Standalone => return Ok(remote.info()),
        SavedMode::Client { .. } => return Err(RemoteError::Connected),
        SavedMode::Unset => {}
    }
    alerts::exit_cleanup(app).await;

    let dir = app.state::<AppDataDir>().0.clone();
    remote
        .update_saved(&dir, |saved| {
            saved.mode = SavedMode::Standalone;
            Ok(true)
        })
        .await?;

    tauri::async_runtime::spawn_blocking(move || {
        crate::startup::config::ensure_desktop_config(&dir, &dir.join("config.toml"))
    })
    .await
    .map_err(|error| RemoteError::Local(error.to_string()))?
    .map_err(RemoteError::Local)?;
    if !app.state::<AppLockState>().locked() {
        crate::core_boot::boot_core(app, BootKind::Launch, false)
            .await
            .map_err(|e| RemoteError::Local(e.to_string()))?;
    }
    crate::host::start_if_enabled(app).await;

    Ok(remote.info())
}

pub(super) async fn pair(
    app: &AppHandle, remote: &RemoteConnection, address: &str, code: Option<&str>,
    allow_private_http: bool,
) -> Result<ConnectionInfo, RemoteError> {
    let (url, code) = parse_pairing(address, code)?;
    let allow = allow_private_http || remote.saved().allow_private_http;
    check_transport(&url, allow)?;
    let name = device_name();
    let hello = ClientHello::Claim {
        code: &code,
        name: &name,
        platform: tauri_plugin_os::platform(),
    };
    let (stream, ok) = open(&url, hello).await?;
    let (Some(token), Some(device_id)) = (ok.token, ok.device_id) else {
        return Err(RemoteError::Unreachable(url.host));
    };

    let next = SavedConnection {
        mode: SavedMode::Client {
            server_url: url.url,
            device_id: device_id.clone(),
            host_kind: ok.host,
            host_name: ok.name,
        },
        allow_private_http: allow,
        alerts_seen: None,
        preferences_pending: false,
    };
    let dir = app.state::<AppDataDir>().0.clone();
    let known_token = remote.lock().token.clone();
    let saved = store::run_blocking(move || {
        store::save_pairing(&dir, &next, &token, known_token).map(|()| (next, token))
    })
    .await;
    let (next, token) = match saved {
        Ok(saved) => saved,
        Err(error) => {
            revoke(stream, &device_id).await;
            return Err(error);
        }
    };

    let (previous, departed) = remote.replace(next, Some(token));
    let old_device = match &previous {
        SavedMode::Client { device_id, .. } => Some(device_id.as_str()),
        _ => None,
    };
    departed.leave(old_device).await;
    if previous == SavedMode::Standalone {
        stop_local_core(app).await;
    }
    remote.launch(app, Some(stream));

    Ok(remote.info())
}

pub(super) async fn disconnect(
    app: &AppHandle, remote: &RemoteConnection,
) -> Result<ConnectionInfo, RemoteError> {
    let saved = remote.saved();
    let SavedMode::Client { device_id, .. } = &saved.mode else {
        return Ok(remote.info());
    };

    let next = SavedConnection {
        mode: SavedMode::Unset,
        allow_private_http: saved.allow_private_http,
        alerts_seen: None,
        preferences_pending: false,
    };
    let dir = app.state::<AppDataDir>().0.clone();
    let next = store::run_blocking(move || store::write(&dir, &next).map(|()| next)).await?;
    let (_, departed) = remote.replace(next, None);
    if let Err(error) = store::run_blocking(store::delete_token).await {
        tracing::warn!(%error, "Couldn't remove the device token");
    }
    departed.leave(Some(device_id)).await;
    alerts::exit_cleanup(app).await;

    Ok(remote.info())
}
