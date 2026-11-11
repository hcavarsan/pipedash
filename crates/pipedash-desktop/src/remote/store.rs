use std::fmt::Display;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::{
    Deserialize,
    Serialize,
};

use super::model::RemoteError;
use crate::credential;

const FILE_NAME: &str = "connection.json";
const TMP_FILE_NAME: &str = "connection.json.tmp";
const KEYCHAIN_SERVICE: &str = "pipedash";
const TOKEN_ACCOUNT: &str = "remote_device_token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    Server,
    Desktop,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SavedMode {
    #[default]
    Unset,
    Standalone,
    Client {
        server_url: String,
        device_id: String,
        host_kind: HostKind,
        host_name: Option<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedConnection {
    #[serde(flatten)]
    pub mode: SavedMode,
    #[serde(default)]
    pub allow_private_http: bool,
    #[serde(default)]
    pub alerts_seen: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preferences_pending: bool,
}

fn storage(error: impl Display) -> RemoteError {
    tracing::warn!(%error, "connection settings file access failed");
    RemoteError::Storage
}

fn keychain(error: impl Display) -> RemoteError {
    tracing::warn!(%error, "remote device token keychain access failed");
    RemoteError::Keychain
}

pub fn read(dir: &Path) -> Option<SavedConnection> {
    let bytes = match fs::read(dir.join(FILE_NAME)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(error = %e, "Couldn't read the saved connection");
            return None;
        }
    };

    serde_json::from_slice(&bytes)
        .inspect_err(|e| tracing::warn!(error = %e, "Ignoring an unreadable saved connection"))
        .ok()
}

pub fn write(dir: &Path, saved: &SavedConnection) -> Result<(), RemoteError> {
    let tmp = dir.join(TMP_FILE_NAME);
    let save = || -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(&tmp, serde_json::to_vec_pretty(saved)?)?;
        fs::rename(&tmp, dir.join(FILE_NAME))
    };

    save().map_err(storage)
}

fn token_entry() -> Result<credential::Entry, RemoteError> {
    credential::device_only_entry(KEYCHAIN_SERVICE, TOKEN_ACCOUNT).map_err(keychain)
}

pub fn get_token() -> Result<Option<String>, RemoteError> {
    match token_entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(credential::Error::NoEntry) => Ok(None),
        Err(e) => Err(keychain(e)),
    }
}

pub fn set_token(token: &str) -> Result<(), RemoteError> {
    token_entry()?.set_password(token).map_err(keychain)
}

pub fn delete_token() -> Result<(), RemoteError> {
    match token_entry()?.delete_credential() {
        Ok(()) | Err(credential::Error::NoEntry) => Ok(()),
        Err(e) => Err(keychain(e)),
    }
}

fn restore_token(previous: Option<&str>) {
    let restored = match previous {
        Some(token) => set_token(token),
        None => delete_token(),
    };
    if let Err(error) = restored {
        tracing::warn!(%error, "Couldn't restore the previous device token");
    }
}

pub fn save_pairing(
    dir: &Path, saved: &SavedConnection, token: &str, known_token: Option<String>,
) -> Result<(), RemoteError> {
    let previous = known_token.or_else(|| get_token().ok().flatten());
    set_token(token)?;
    write(dir, saved).inspect_err(|_| restore_token(previous.as_deref()))
}

pub async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, RemoteError> + Send + 'static,
) -> Result<T, RemoteError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| RemoteError::Storage)?
}

#[cfg(test)]
mod tests;
