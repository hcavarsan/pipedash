use std::path::PathBuf;
use std::sync::OnceLock;

use base64::{
    engine::general_purpose::STANDARD,
    Engine,
};

use super::super::file_vault::{
    Exclusive,
    FileVault,
};
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
    let vault =
        open(true)?.ok_or_else(|| error("Background notification credentials are unavailable"))?;
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

pub(super) fn exclusive() -> Result<Exclusive> {
    open(true)?
        .ok_or_else(|| error("Background notification credentials are unavailable"))?
        .exclusive()
        .map_err(|_| error("Background notification storage is unavailable"))
}
