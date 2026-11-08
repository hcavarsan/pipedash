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
