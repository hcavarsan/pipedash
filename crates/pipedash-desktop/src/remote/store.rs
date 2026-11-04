use std::fmt::Display;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::{
    Deserialize,
    Serialize,
};

use super::RemoteError;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> SavedConnection {
        SavedConnection {
            mode: SavedMode::Client {
                server_url: "http://192.168.1.20:8080".to_string(),
                device_id: "d1".to_string(),
                host_kind: HostKind::Desktop,
                host_name: Some("Studio".to_string()),
            },
            allow_private_http: true,
            alerts_seen: Some(7),
            preferences_pending: true,
        }
    }

    #[test]
    fn every_mode_round_trips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        for saved in [
            SavedConnection::default(),
            SavedConnection {
                mode: SavedMode::Standalone,
                allow_private_http: false,
                alerts_seen: None,
                preferences_pending: false,
            },
            client(),
        ] {
            write(dir.path(), &saved).unwrap();
            assert_eq!(read(dir.path()), Some(saved));
        }
        assert!(!dir.path().join(TMP_FILE_NAME).exists());
    }

    #[test]
    fn the_file_is_flat_and_holds_no_token() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &client()).unwrap();

        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join(FILE_NAME)).unwrap()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "mode": "client",
                "server_url": "http://192.168.1.20:8080",
                "device_id": "d1",
                "host_kind": "desktop",
                "host_name": "Studio",
                "allow_private_http": true,
                "alerts_seen": 7,
                "preferences_pending": true,
            })
        );
    }

    #[test]
    fn a_missing_or_unreadable_file_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(dir.path()), None);

        fs::write(dir.path().join(FILE_NAME), b"{not json").unwrap();
        assert_eq!(read(dir.path()), None);
    }
}
