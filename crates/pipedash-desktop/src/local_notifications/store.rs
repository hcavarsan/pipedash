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

#[cfg(all(not(target_os = "ios"), not(test)))]
mod vault {
    use super::{
        error,
        Result,
    };

    pub(super) fn get(_account: &str) -> Result<Option<String>> {
        Err(error("Background notifications require iOS"))
    }

    pub(super) fn set(_account: &str, _value: &str) -> Result<()> {
        Err(error("Background notifications require iOS"))
    }

    pub(super) fn remove(_account: &str) -> Result<()> {
        Err(error("Background notifications require iOS"))
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
