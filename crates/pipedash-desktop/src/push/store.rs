use std::fmt::Display;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::Deserialize;

use super::{
    PushError,
    Result,
};

const FILE_NAME: &str = "push_servers.json";

#[derive(Debug, Default, Deserialize)]
pub struct PushFile {
    #[serde(default)]
    pub servers: Vec<ServerRecord>,
}

#[derive(Debug, Deserialize)]
pub struct ServerRecord {
    pub id: String,
}

#[derive(Clone, Copy)]
pub enum Secret {
    ApiToken,
    RelaySubscription,
}

impl Secret {
    fn user(self, id: &str) -> String {
        match self {
            Self::ApiToken => format!("push_server_token_{id}"),
            Self::RelaySubscription => format!("push_relay_subscription_{id}"),
        }
    }
}

fn storage(error: impl Display) -> PushError {
    tracing::warn!(%error, "push settings file access failed");
    PushError::Storage
}

fn keychain(error: impl Display) -> PushError {
    tracing::warn!(%error, "push keychain access failed");
    PushError::Keychain
}

pub fn read(dir: &Path) -> Result<PushFile> {
    match fs::read(dir.join(FILE_NAME)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(storage),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(PushFile::default()),
        Err(e) => Err(storage(e)),
    }
}

pub fn remove_file(dir: &Path) -> Result<()> {
    match fs::remove_file(dir.join(FILE_NAME)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(storage(e)),
    }
}

#[cfg(not(test))]
mod vault {
    use crate::credential;

    const SERVICE: &str = "pipedash";

    pub fn get(user: &str) -> Result<Option<String>, String> {
        let entry = credential::device_only_entry(SERVICE, user).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(credential::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn delete(user: &str) -> Result<(), String> {
        let entry = credential::device_only_entry(SERVICE, user).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(credential::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod vault {
    use std::collections::HashMap;
    use std::sync::{
        Mutex,
        PoisonError,
    };

    static SECRETS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

    fn with<T>(change: impl FnOnce(&mut HashMap<String, String>) -> T) -> T {
        let mut guard = SECRETS.lock().unwrap_or_else(PoisonError::into_inner);
        change(guard.get_or_insert_with(HashMap::new))
    }

    pub fn set(user: &str, value: &str) {
        with(|secrets| secrets.insert(user.to_owned(), value.to_owned()));
    }

    pub fn get(user: &str) -> Result<Option<String>, String> {
        Ok(with(|secrets| secrets.get(user).cloned()))
    }

    pub fn delete(user: &str) -> Result<(), String> {
        with(|secrets| secrets.remove(user));
        Ok(())
    }
}

#[cfg(test)]
pub fn set_secret(kind: Secret, id: &str, value: &str) {
    vault::set(&kind.user(id), value);
}

pub fn get_secret(kind: Secret, id: &str) -> Result<Option<String>> {
    vault::get(&kind.user(id)).map_err(keychain)
}

pub fn forget(id: &str) -> Result<()> {
    for kind in [Secret::ApiToken, Secret::RelaySubscription] {
        vault::delete(&kind.user(id)).map_err(keychain)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_users_are_per_server() {
        assert_eq!(Secret::ApiToken.user("ab"), "push_server_token_ab");
        assert_eq!(
            Secret::RelaySubscription.user("ab"),
            "push_relay_subscription_ab"
        );
    }

    #[test]
    fn old_files_with_extra_fields_still_read() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path()).unwrap().servers.is_empty());

        fs::write(
            dir.path().join(FILE_NAME),
            r#"{"servers":[{"id":"a","url":"https://ci.example.com","label":"CI","paired_at":"2026-01-01T00:00:00Z"}]}"#,
        )
        .unwrap();
        assert_eq!(read(dir.path()).unwrap().servers[0].id, "a");

        remove_file(dir.path()).unwrap();
        remove_file(dir.path()).unwrap();
        assert!(read(dir.path()).unwrap().servers.is_empty());
    }
}
