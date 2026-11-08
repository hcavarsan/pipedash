use serde::{
    Deserialize,
    Serialize,
};

use crate::push::{
    PushError,
    Result,
};

#[derive(Serialize, Deserialize)]
pub(super) struct Registration {
    pub(super) subscription_id: String,
    pub(super) device_id: String,
}

fn keychain(error: impl std::fmt::Display) -> PushError {
    tracing::warn!(%error, "push keychain access failed");
    PushError::Keychain
}

#[cfg(not(test))]
mod saved {
    use super::{
        keychain,
        Result,
    };
    use crate::credential;

    const KEYCHAIN_SERVICE: &str = "pipedash";
    const KEYCHAIN_ACCOUNT: &str = "device_push";

    fn entry() -> Result<credential::Entry> {
        credential::device_only_entry(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(keychain)
    }

    pub(super) fn read() -> Result<Option<String>> {
        match entry()?.get_password() {
            Ok(saved) => Ok(Some(saved)),
            Err(credential::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain(error)),
        }
    }

    pub(super) fn write(value: &str) -> Result<()> {
        entry()?.set_password(value).map_err(keychain)
    }

    pub(super) fn delete() -> Result<()> {
        match entry()?.delete_credential() {
            Ok(()) | Err(credential::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain(error)),
        }
    }
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(keychain)?
}

pub(super) async fn registration() -> Result<Option<Registration>> {
    blocking(|| Ok(saved::read()?.and_then(|saved| serde_json::from_str(&saved).ok()))).await
}

pub(super) async fn save_registration(registration: Registration) -> Result<()> {
    blocking(move || saved::write(&serde_json::to_string(&registration).map_err(keychain)?)).await
}

pub(super) async fn delete_registration() -> Result<()> {
    blocking(saved::delete).await
}

#[cfg(test)]
mod saved {
    use std::sync::{
        Mutex,
        MutexGuard,
        PoisonError,
    };

    use super::Result;

    static SAVED: Mutex<Option<String>> = Mutex::new(None);

    fn slot() -> MutexGuard<'static, Option<String>> {
        SAVED.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn read() -> Result<Option<String>> {
        Ok(slot().clone())
    }

    pub(super) fn write(value: &str) -> Result<()> {
        *slot() = Some(value.to_owned());
        Ok(())
    }

    pub(super) fn delete() -> Result<()> {
        *slot() = None;
        Ok(())
    }
}
