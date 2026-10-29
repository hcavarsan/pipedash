use pipedash_core::application::NotificationState;
use pipedash_core::domain::ProviderConfig;
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};

use super::{
    error,
    Result,
};

const AUTHORIZATION: &str = "authorized-providers";
const OBSERVATIONS: &str = "observations";

#[derive(Serialize, Deserialize)]
pub(super) struct Authorization {
    pub providers: Vec<ProviderConfig>,
    pub proxy_url: String,
    pub status_pages_enabled: bool,
}

pub(super) fn authorization() -> Result<Option<Authorization>> {
    read(AUTHORIZATION)
}

pub(super) fn observations() -> Result<NotificationState> {
    Ok(read(OBSERVATIONS)?.unwrap_or_default())
}

pub(super) fn save_authorization(value: &Authorization) -> Result<()> {
    write(AUTHORIZATION, value)
}

pub(super) fn save_observations(value: &NotificationState) -> Result<()> {
    write(OBSERVATIONS, value)
}

pub(super) fn clear() -> Result<()> {
    let authorization = vault::remove(AUTHORIZATION);
    let observations = vault::remove(OBSERVATIONS);
    authorization?;
    observations
}

fn read<T: DeserializeOwned>(account: &str) -> Result<Option<T>> {
    vault::get(account)?
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|_| error("Saved background notification data could not be read"))
        })
        .transpose()
}

fn write(account: &str, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_string(value)
        .map_err(|_| error("Background notification data could not be encoded"))?;
    vault::set(account, &data)
}

#[cfg(all(target_os = "android", not(test)))]
pub(super) fn set_dir(dir: std::path::PathBuf) {
    vault::set_dir(dir);
}

#[cfg(all(target_os = "android", not(test)))]
pub(super) fn generation() -> Result<u64> {
    vault::generation()
}

#[cfg(all(target_os = "android", not(test)))]
pub(super) fn save_observations_if_unchanged(
    value: &NotificationState, generation: u64,
) -> Result<bool> {
    let data = serde_json::to_string(value)
        .map_err(|_| error("Background notification data could not be encoded"))?;
    vault::set_if_generation(OBSERVATIONS, &data, generation)
}

#[cfg(all(target_os = "ios", not(test)))]
mod vault {
    use super::{
        error,
        Result,
    };
    use crate::credential;

    const SERVICE: &str = "pipedash.background-notifications";

    pub(super) fn get(account: &str) -> Result<Option<String>> {
        let entry = credential::background_entry(SERVICE, account)
            .map_err(|_| error("Background notification credentials are unavailable"))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(credential::Error::NoEntry) => Ok(None),
            Err(_) => Err(error(
                "Unlock the device before accessing background notification credentials",
            )),
        }
    }

    pub(super) fn set(account: &str, value: &str) -> Result<()> {
        credential::background_entry(SERVICE, account)
            .and_then(|entry| entry.set_password(value))
            .map_err(|_| error("Background notification credentials could not be saved"))
    }

    pub(super) fn remove(account: &str) -> Result<()> {
        let entry = credential::background_entry(SERVICE, account)
            .map_err(|_| error("Background notification credentials are unavailable"))?;
        match entry.delete_credential() {
            Ok(()) | Err(credential::Error::NoEntry) => Ok(()),
            Err(_) => Err(error(
                "Background notification credentials could not be removed",
            )),
        }
    }
}

#[cfg(all(target_os = "android", not(test)))]
mod vault {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use base64::{
        engine::general_purpose::STANDARD,
        Engine,
    };

    use super::super::file_vault::FileVault;
    use super::{
        error,
        Result,
    };
    use crate::credential;

    const SERVICE: &str = "pipedash.background-notifications";
    const KEY_ACCOUNT: &str = "file-key";

    static DIR: OnceLock<PathBuf> = OnceLock::new();

    pub(super) fn set_dir(dir: PathBuf) {
        let _ = DIR.set(dir);
    }

    fn key(create: bool) -> Result<Option<[u8; 32]>> {
        let entry = credential::background_entry(SERVICE, KEY_ACCOUNT)
            .map_err(|_| error("Background notification credentials are unavailable"))?;
        match entry.get_password() {
            Ok(text) => STANDARD
                .decode(text)
                .ok()
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                .map(Some)
                .ok_or_else(|| error("Saved background notification data could not be read")),
            Err(credential::Error::NoEntry) if create => {
                let key: [u8; 32] = rand::random();
                entry
                    .set_password(&STANDARD.encode(key))
                    .map_err(|_| error("Background notification credentials could not be saved"))?;
                Ok(Some(key))
            }
            Err(credential::Error::NoEntry) => Ok(None),
            Err(_) => Err(error("Background notification credentials are unavailable")),
        }
    }

    fn open(create: bool) -> Result<Option<FileVault>> {
        let dir = DIR
            .get()
            .ok_or_else(|| error("Background notification storage is unavailable"))?;
        Ok(key(create)?.map(|key| FileVault::new(dir.clone(), key)))
    }

    pub(super) fn get(account: &str) -> Result<Option<String>> {
        match open(false)? {
            Some(vault) => vault
                .get(account)
                .map_err(|_| error("Saved background notification data could not be read")),
            None => Ok(None),
        }
    }

    pub(super) fn set(account: &str, value: &str) -> Result<()> {
        let vault = open(true)?
            .ok_or_else(|| error("Background notification credentials are unavailable"))?;
        vault
            .set(account, value)
            .map_err(|_| error("Background notification credentials could not be saved"))
    }

    pub(super) fn remove(account: &str) -> Result<()> {
        match open(false)? {
            Some(vault) => vault
                .remove(account)
                .map_err(|_| error("Background notification credentials could not be removed")),
            None => Ok(()),
        }
    }

    pub(super) fn generation() -> Result<u64> {
        match open(false)? {
            Some(vault) => vault
                .generation()
                .map_err(|_| error("Saved background notification data could not be read")),
            None => Ok(0),
        }
    }

    pub(super) fn set_if_generation(account: &str, value: &str, expected: u64) -> Result<bool> {
        match open(false)? {
            Some(vault) => vault
                .set_if_generation(account, value, expected)
                .map_err(|_| error("Background notification credentials could not be saved")),
            None => Ok(false),
        }
    }
}

#[cfg(all(not(any(target_os = "ios", target_os = "android")), not(test)))]
mod vault {
    use super::{
        error,
        Result,
    };

    pub(super) fn get(_account: &str) -> Result<Option<String>> {
        Err(error(
            "Background notifications require the Pipedash mobile app",
        ))
    }

    pub(super) fn set(_account: &str, _value: &str) -> Result<()> {
        Err(error(
            "Background notifications require the Pipedash mobile app",
        ))
    }

    pub(super) fn remove(_account: &str) -> Result<()> {
        Err(error(
            "Background notifications require the Pipedash mobile app",
        ))
    }
}

#[cfg(test)]
mod vault {
    use std::collections::HashMap;
    use std::sync::{
        LazyLock,
        Mutex,
    };

    use super::Result;

    static ITEMS: LazyLock<Mutex<HashMap<String, String>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    pub(super) fn get(account: &str) -> Result<Option<String>> {
        Ok(ITEMS.lock().unwrap().get(account).cloned())
    }

    pub(super) fn set(account: &str, value: &str) -> Result<()> {
        ITEMS
            .lock()
            .unwrap()
            .insert(account.to_owned(), value.to_owned());
        Ok(())
    }

    pub(super) fn remove(account: &str) -> Result<()> {
        ITEMS.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_removes_authorized_credentials_and_observations() {
        clear().unwrap();
        assert!(authorization().unwrap().is_none());
        let provider = ProviderConfig {
            id: Some(7),
            name: "CI".to_owned(),
            provider_type: "github".to_owned(),
            token: "authorized-test-token".to_owned(),
            config: Default::default(),
            refresh_interval: 30,
            version: None,
        };
        save_authorization(&Authorization {
            providers: vec![provider],
            proxy_url: String::new(),
            status_pages_enabled: true,
        })
        .unwrap();
        save_observations(&NotificationState::default()).unwrap();
        assert_eq!(
            authorization().unwrap().unwrap().providers[0].token,
            "authorized-test-token"
        );
        clear().unwrap();
        assert!(authorization().unwrap().is_none());
        assert!(vault::get(OBSERVATIONS).unwrap().is_none());
    }
}
