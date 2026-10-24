use std::fmt::Display;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::{
    Mutex,
    PoisonError,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    PushError,
    Result,
};

const FILE_NAME: &str = "push_servers.json";
const TMP_FILE_NAME: &str = "push_servers.json.tmp";

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PushFile {
    #[serde(default)]
    pub servers: Vec<ServerRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRecord {
    pub id: String,
    pub url: String,
    pub label: String,
    pub paired_at: DateTime<Utc>,
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

fn load(dir: &Path) -> Result<PushFile> {
    match fs::read(dir.join(FILE_NAME)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(storage),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(PushFile::default()),
        Err(e) => Err(storage(e)),
    }
}

fn save(dir: &Path, file: &PushFile) -> Result<()> {
    fs::create_dir_all(dir).map_err(storage)?;
    let tmp = dir.join(TMP_FILE_NAME);
    let bytes = serde_json::to_vec_pretty(file).map_err(storage)?;
    fs::write(&tmp, bytes).map_err(storage)?;
    fs::rename(&tmp, dir.join(FILE_NAME)).map_err(storage)
}

pub fn read(dir: &Path) -> Result<PushFile> {
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    load(dir)
}

pub fn update<T>(dir: &Path, change: impl FnOnce(&mut PushFile) -> T) -> Result<T> {
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut file = load(dir)?;
    let out = change(&mut file);
    save(dir, &file)?;
    Ok(out)
}

pub fn remove_file(dir: &Path) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
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

    pub fn set(user: &str, value: &str) -> Result<(), String> {
        credential::device_only_entry(SERVICE, user)
            .and_then(|entry| entry.set_password(value))
            .map_err(|e| e.to_string())
    }

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

    pub fn set(user: &str, value: &str) -> Result<(), String> {
        with(|secrets| secrets.insert(user.to_owned(), value.to_owned()));
        Ok(())
    }

    pub fn get(user: &str) -> Result<Option<String>, String> {
        Ok(with(|secrets| secrets.get(user).cloned()))
    }

    pub fn delete(user: &str) -> Result<(), String> {
        with(|secrets| secrets.remove(user));
        Ok(())
    }
}

pub fn set_secret(kind: Secret, id: &str, value: &str) -> Result<()> {
    vault::set(&kind.user(id), value).map_err(keychain)
}

pub fn get_secret(kind: Secret, id: &str) -> Result<Option<String>> {
    vault::get(&kind.user(id)).map_err(keychain)
}

pub fn forget(id: &str) {
    for kind in [Secret::ApiToken, Secret::RelaySubscription] {
        if let Err(error) = vault::delete(&kind.user(id)) {
            tracing::warn!(%error, "push keychain cleanup failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn keys(value: &serde_json::Value) -> BTreeSet<&str> {
        value
            .as_object()
            .map(|o| o.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    #[test]
    fn round_trip_holds_no_secret_fields() {
        let file = PushFile {
            servers: vec![ServerRecord {
                id: "id1".to_owned(),
                url: "https://ci.example.com".to_owned(),
                label: "CI".to_owned(),
                paired_at: Utc::now(),
            }],
        };
        let json = serde_json::to_string(&file).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(keys(&value), BTreeSet::from(["servers"]));
        assert_eq!(
            keys(&value["servers"][0]),
            BTreeSet::from(["id", "url", "label", "paired_at"])
        );
        assert_eq!(serde_json::from_str::<PushFile>(&json).unwrap(), file);
    }

    #[test]
    fn missing_fields_default() {
        let file: PushFile = serde_json::from_str("{}").unwrap();
        assert_eq!(file, PushFile::default());
    }

    #[test]
    fn secret_users_are_per_server() {
        assert_eq!(Secret::ApiToken.user("ab"), "push_server_token_ab");
        assert_eq!(
            Secret::RelaySubscription.user("ab"),
            "push_relay_subscription_ab"
        );
    }
}
