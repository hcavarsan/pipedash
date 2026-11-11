use std::sync::{
    Mutex,
    PoisonError,
};

use pipedash_core::application::NotificationState;
use pipedash_core::domain::{
    NotificationPreferences,
    ProviderConfig,
    RunFinish,
};
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
const ALERT_HOST: &str = "alert-host";
const PREFERENCES: &str = "notification-preferences";

static ALERT_HOST_LOCK: Mutex<()> = Mutex::new(());

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

pub(super) fn preferences() -> Result<NotificationPreferences> {
    Ok(read(PREFERENCES)?.unwrap_or_default())
}

pub(super) fn fire_once(finishes: &[(String, RunFinish)]) -> Result<()> {
    update_preferences(|preferences| {
        finishes.iter().fold(false, |fired, (pipeline_id, finish)| {
            !preferences.fire_once(pipeline_id, finish).is_empty() | fired
        })
    })
    .map(drop)
}

pub(super) fn merge_preferences(
    incoming: &NotificationPreferences,
) -> Result<(NotificationPreferences, bool)> {
    update_preferences(|stored| stored.merge(incoming))
}

#[cfg(all(target_os = "android", not(test)))]
fn update_preferences(
    change: impl FnOnce(&mut NotificationPreferences) -> bool,
) -> Result<(NotificationPreferences, bool)> {
    let session = vault::exclusive()?;
    let mut preferences = read_locked(&session, PREFERENCES)
        .ok()
        .flatten()
        .unwrap_or_default();
    let changed = change(&mut preferences);
    if changed {
        let data = serde_json::to_string(&preferences)
            .map_err(|_| error("Background notification data could not be encoded"))?;
        session
            .set(PREFERENCES, &data)
            .map_err(|_| error("Background notification credentials could not be saved"))?;
    }

    Ok((preferences, changed))
}

#[cfg(not(all(target_os = "android", not(test))))]
fn update_preferences(
    change: impl FnOnce(&mut NotificationPreferences) -> bool,
) -> Result<(NotificationPreferences, bool)> {
    let mut preferences = preferences().unwrap_or_default();
    let changed = change(&mut preferences);
    if changed {
        write(PREFERENCES, &preferences)?;
    }

    Ok((preferences, changed))
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AlertHost {
    pub server_url: String,
    pub token: String,
    pub allow_private_http: bool,
    pub device_id: String,
    pub host_name: String,
    pub standalone: bool,
    pub notified_seq: i64,
    pub push: bool,
}

pub(super) fn alert_host() -> Result<Option<AlertHost>> {
    read(ALERT_HOST)
}

pub(super) fn save_alert_host(value: &AlertHost) -> Result<()> {
    write(ALERT_HOST, value)
}

pub(super) fn remove_alert_host_record() -> Result<()> {
    vault::remove(ALERT_HOST)
}

fn update_alert_host_if(
    matches: impl FnOnce(&AlertHost) -> bool, change: impl FnOnce(&mut AlertHost),
) -> Result<bool> {
    let _guard = ALERT_HOST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let Some(mut stored) = alert_host()? else {
        return Ok(false);
    };
    if !matches(&stored) {
        return Ok(false);
    }
    change(&mut stored);
    save_alert_host(&stored)?;

    Ok(true)
}

fn same_host(expected: &AlertHost) -> impl Fn(&AlertHost) -> bool + '_ {
    move |stored| stored.device_id == expected.device_id && stored.server_url == expected.server_url
}

pub(super) fn advance_alert_host(seq: i64) -> Result<()> {
    update_alert_host_if(
        |_| true,
        |host| {
            host.notified_seq = host.notified_seq.max(seq);
        },
    )
    .map(drop)
}

pub(super) fn reset_alert_host_seq(seq: i64) -> Result<()> {
    update_alert_host_if(|_| true, |host| host.notified_seq = seq).map(drop)
}

pub(super) fn set_alert_host_push(expected: &AlertHost, push: bool) -> Result<bool> {
    update_alert_host_if(same_host(expected), |host| host.push = push)
}

pub(super) fn advance_alert_host_for(expected: &AlertHost, seq: i64) -> Result<bool> {
    update_alert_host_if(same_host(expected), |host| {
        host.notified_seq = host.notified_seq.max(seq);
    })
}

pub(super) fn reset_alert_host_seq_for(expected: &AlertHost, seq: i64) -> Result<bool> {
    update_alert_host_if(same_host(expected), |host| host.notified_seq = seq)
}

pub(super) fn is_alert_host(expected: &AlertHost) -> Result<bool> {
    Ok(alert_host()?.is_some_and(|stored| same_host(expected)(&stored)))
}

pub(super) fn clear() -> Result<()> {
    let authorization = vault::remove(AUTHORIZATION);
    let observations = vault::remove(OBSERVATIONS);
    let alert_host = vault::remove(ALERT_HOST);
    authorization?;
    observations?;
    alert_host
}

fn read<T: DeserializeOwned>(account: &str) -> Result<Option<T>> {
    decode(vault::get(account)?)
}

fn decode<T: DeserializeOwned>(value: Option<String>) -> Result<Option<T>> {
    value
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

#[cfg(all(target_os = "android", not(test)))]
pub(super) struct ObservationsLock(super::file_vault::Exclusive);

#[cfg(all(target_os = "android", not(test)))]
pub(super) fn lock_observations(
) -> Result<(ObservationsLock, NotificationState, NotificationPreferences)> {
    let session = vault::exclusive()?;
    let observations = read_locked(&session, OBSERVATIONS)?.unwrap_or_default();
    let preferences = read_locked(&session, PREFERENCES)?.unwrap_or_default();
    Ok((ObservationsLock(session), observations, preferences))
}

#[cfg(all(target_os = "android", not(test)))]
fn read_locked<T: DeserializeOwned>(
    session: &super::file_vault::Exclusive, account: &str,
) -> Result<Option<T>> {
    decode(
        session
            .get(account)
            .map_err(|_| error("Saved background notification data could not be read"))?,
    )
}

#[cfg(all(target_os = "android", not(test)))]
impl ObservationsLock {
    pub(super) fn save(self, value: &NotificationState) -> Result<()> {
        let data = serde_json::to_string(value)
            .map_err(|_| error("Background notification data could not be encoded"))?;
        self.0
            .set(OBSERVATIONS, &data)
            .map_err(|_| error("Background notification credentials could not be saved"))
    }
}

#[cfg(not(test))]
#[cfg_attr(target_os = "ios", path = "store/ios.rs")]
#[cfg_attr(target_os = "android", path = "store/android.rs")]
#[cfg_attr(
    not(any(target_os = "ios", target_os = "android")),
    path = "store/unsupported.rs"
)]
mod vault;

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
mod tests;
