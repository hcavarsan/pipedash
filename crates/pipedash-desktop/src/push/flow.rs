use serde::{
    Deserialize,
    Serialize,
};
use tauri::AppHandle;
use tauri_plugin_pipedash_push::PipedashPushExt;

use super::{
    host_failure,
    host_problem,
    relay,
    PushError,
    Result,
    HOST_PUSH_PATH,
    RELAY_NOT_CONFIGURED_CODE,
    UNAVAILABLE,
};
use crate::local_notifications::{
    self,
    AlertHost,
};
use crate::remote::alerts::host_request;

pub(super) static PUSH_OPS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) trait Env {
    fn relay_url(&self) -> &str;
    async fn notifications_enabled(&self) -> bool;
    async fn stored_host(&self) -> Option<AlertHost>;
    async fn available(&self) -> bool;
    async fn permission_granted(&self) -> bool;
    async fn device_token(&self) -> Result<String>;
    async fn mark_push(&self, host: &AlertHost, push: bool);
    fn set_problem(&self, problem: Option<String>);
}

pub(super) struct AppEnv<'a>(pub &'a AppHandle);

impl Env for AppEnv<'_> {
    fn relay_url(&self) -> &str {
        relay::base_url()
    }

    async fn notifications_enabled(&self) -> bool {
        local_notifications::notifications_enabled(self.0).await
    }

    async fn stored_host(&self) -> Option<AlertHost> {
        local_notifications::current_alert_host().await
    }

    async fn available(&self) -> bool {
        cfg!(target_os = "ios") && matches!(self.0.pipedash_push().is_enabled().await, Ok(true))
    }

    async fn permission_granted(&self) -> bool {
        permission_granted(self.0).await
    }

    async fn device_token(&self) -> Result<String> {
        acquire_device_token(self.0).await
    }

    async fn mark_push(&self, host: &AlertHost, push: bool) {
        local_notifications::mark_alert_host_push(host, push).await;
    }

    fn set_problem(&self, problem: Option<String>) {
        local_notifications::set_push_problem(self.0, problem);
    }
}

#[cfg(target_os = "ios")]
async fn permission_granted(app: &AppHandle) -> bool {
    use tauri_plugin_notification::NotificationExt;

    let app = app.clone();
    matches!(
        tauri::async_runtime::spawn_blocking(move || app.notification().permission_state()).await,
        Ok(Ok(tauri::plugin::PermissionState::Granted))
    )
}

#[cfg(not(target_os = "ios"))]
async fn permission_granted(_app: &AppHandle) -> bool {
    false
}

#[cfg(target_os = "ios")]
async fn acquire_device_token(app: &AppHandle) -> Result<String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.pipedash_push().register_for_remote_notifications()
    })
    .await
    .map_err(|_| PushError::Registration)?
    .map_err(PushError::from)
}

#[cfg(not(target_os = "ios"))]
async fn acquire_device_token(_app: &AppHandle) -> Result<String> {
    Err(PushError::Unsupported)
}

#[derive(Serialize, Deserialize)]
struct Registration {
    subscription_id: String,
    device_id: String,
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

#[cfg(test)]
mod saved {
    use std::cell::RefCell;

    use super::Result;

    thread_local! {
        static SAVED: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    pub(super) fn read() -> Result<Option<String>> {
        Ok(SAVED.with_borrow(Clone::clone))
    }

    pub(super) fn write(value: &str) -> Result<()> {
        SAVED.set(Some(value.to_owned()));
        Ok(())
    }

    pub(super) fn delete() -> Result<()> {
        SAVED.set(None);
        Ok(())
    }
}

fn registration() -> Result<Option<Registration>> {
    Ok(saved::read()?.and_then(|saved| serde_json::from_str(&saved).ok()))
}

fn save_registration(registration: &Registration) -> Result<()> {
    saved::write(&serde_json::to_string(registration).map_err(keychain)?)
}

async fn revoke_relay(relay_url: &str, subscription_id: &str) {
    if let Err(error) = relay::delete(relay_url, subscription_id).await {
        tracing::warn!(%error, "could not revoke the push relay subscription");
    }
}

enum RegisterError {
    HostStopped,
    Failed(String),
}

impl RegisterError {
    fn problem(self, host: &AlertHost) -> String {
        match self {
            Self::HostStopped => no_push_problem(host),
            Self::Failed(problem) => problem,
        }
    }
}

fn no_push_problem(host: &AlertHost) -> String {
    format!(
        "{} doesn't send push alerts. Its owner can turn them on in Settings → Devices.",
        host.host_name
    )
}

fn failed(error: impl std::fmt::Display) -> RegisterError {
    RegisterError::Failed(error.to_string())
}

async fn register(
    relay_url: &str, host: &AlertHost, device_token: &str, saved: Option<Registration>,
) -> Result<(), RegisterError> {
    let reused = match saved {
        Some(saved) if saved.device_id == host.device_id => Some(saved.subscription_id),
        Some(other) => {
            revoke_relay(relay_url, &other.subscription_id).await;
            None
        }
        None => None,
    };
    let rotated = match reused {
        Some(subscription_id) => {
            match relay::rotate(relay_url, &subscription_id, device_token).await {
                Ok(()) => Some(subscription_id),
                Err(PushError::RelayGone) => None,
                Err(error) => return Err(failed(error)),
            }
        }
        None => None,
    };
    let fresh = rotated.is_none();
    let subscription_id = match rotated {
        Some(subscription_id) => subscription_id,
        None => relay::create(relay_url, device_token)
            .await
            .map_err(failed)?,
    };

    let body = serde_json::json!({ "subscription_id": subscription_id }).to_string();
    let accepted = match host_request(host, "PUT", HOST_PUSH_PATH, Some(&body)).await {
        Ok((204, _)) => Ok(()),
        Ok((410, _)) => Err(RegisterError::HostStopped),
        Ok((status, body)) => {
            if host_failure(&body).code.as_deref() == Some(RELAY_NOT_CONFIGURED_CODE) {
                Err(RegisterError::HostStopped)
            } else {
                Err(RegisterError::Failed(host_problem(host, status, &body)))
            }
        }
        Err(error) => Err(failed(error)),
    };
    let saved = accepted.and_then(|()| {
        save_registration(&Registration {
            subscription_id: subscription_id.clone(),
            device_id: host.device_id.clone(),
        })
        .map_err(failed)
    });
    if let Err(error) = saved {
        if fresh {
            revoke_relay(relay_url, &subscription_id).await;
        }
        return Err(error);
    }

    Ok(())
}

#[derive(Deserialize)]
struct HostPushBody {
    relay_url: Option<String>,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    undelivered_after: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostPush {
    Active { undelivered_after: Option<i64> },
    NoRegistration,
    RelayStopped,
}

fn classify(body: &HostPushBody, relay_url: &str) -> HostPush {
    match body
        .relay_url
        .as_deref()
        .map(|url| url.trim_end_matches('/'))
    {
        Some(url) if url == relay_url && body.enabled => HostPush::Active {
            undelivered_after: body.undelivered_after,
        },
        Some(url) if url == relay_url => HostPush::NoRegistration,
        _ => HostPush::RelayStopped,
    }
}

pub(crate) async fn host_push(host: &AlertHost, relay_url: &str) -> Option<HostPush> {
    let (status, body) = host_request(host, "GET", HOST_PUSH_PATH, None).await.ok()?;
    if status != 200 {
        return None;
    }

    serde_json::from_str::<HostPushBody>(&body)
        .ok()
        .map(|body| classify(&body, relay_url))
}

async fn active_host(env: &impl Env) -> Option<AlertHost> {
    if !env.notifications_enabled().await {
        return None;
    }

    env.stored_host().await
}

fn same_host(a: &AlertHost, b: &AlertHost) -> bool {
    a.device_id == b.device_id && a.server_url == b.server_url
}

pub(super) async fn enable_with(env: &impl Env) -> Result<(), String> {
    if !env.available().await {
        return Err(UNAVAILABLE.to_owned());
    }
    let host = active_host(env)
        .await
        .ok_or_else(|| "Turn on notifications first.".to_owned())?;

    let (status, body) = host_request(&host, "GET", HOST_PUSH_PATH, None)
        .await
        .map_err(|error| error.to_string())?;
    if status != 200 {
        return Err(host_problem(&host, status, &body));
    }
    let relay_url = serde_json::from_str::<HostPushBody>(&body)
        .ok()
        .and_then(|push| push.relay_url)
        .ok_or_else(|| no_push_problem(&host))?;
    if relay_url.trim_end_matches('/') != env.relay_url() {
        return Err(format!(
            "{} sends push alerts through a different relay.",
            host.host_name
        ));
    }

    let device_token = env
        .device_token()
        .await
        .map_err(|error| error.to_string())?;
    register(
        env.relay_url(),
        &host,
        &device_token,
        registration().ok().flatten(),
    )
    .await
    .map_err(|error| error.problem(&host))?;
    env.mark_push(&host, true).await;

    Ok(())
}

pub(super) async fn forget_at(relay_url: &str) {
    match registration() {
        Ok(Some(saved)) => revoke_relay(relay_url, &saved.subscription_id).await,
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "could not read the push registration"),
    }
    if let Err(error) = saved::delete() {
        tracing::warn!(%error, "could not remove the push registration");
    }
}

pub(super) async fn disable_with(env: &impl Env) {
    let host = env.stored_host().await;
    if let Some(host) = &host {
        if let Err(error) = host_request(host, "DELETE", HOST_PUSH_PATH, None).await {
            tracing::debug!(%error, "Couldn't tell the alert host to stop push alerts");
        }
    }
    forget_at(env.relay_url()).await;
    if let Some(host) = &host {
        env.mark_push(host, false).await;
    }
}

async fn host_stopped(env: &impl Env, host: &AlertHost) {
    forget_at(env.relay_url()).await;
    env.mark_push(host, false).await;
    env.set_problem(Some(format!(
        "{} stopped sending push alerts. Pipedash checks it in the background instead.",
        host.host_name
    )));
}

async fn refresh_with(env: &impl Env, host: &AlertHost) {
    if host_push(host, env.relay_url()).await == Some(HostPush::RelayStopped) {
        host_stopped(env, host).await;
        return;
    }
    let device_token = match env.device_token().await {
        Ok(token) => token,
        Err(error) => {
            tracing::debug!(%error, "Couldn't read the push token");
            return;
        }
    };
    match register(
        env.relay_url(),
        host,
        &device_token,
        registration().ok().flatten(),
    )
    .await
    {
        Ok(()) => env.mark_push(host, true).await,
        Err(RegisterError::HostStopped) => host_stopped(env, host).await,
        Err(RegisterError::Failed(_)) => tracing::debug!("Couldn't refresh the push registration"),
    }
}

async fn setup_with(env: &impl Env, host: &AlertHost) {
    let offered = host_push(host, env.relay_url())
        .await
        .is_some_and(|state| state != HostPush::RelayStopped);
    if !offered {
        return;
    }
    match enable_with(env).await {
        Ok(()) => env.set_problem(None),
        Err(problem) => env.set_problem(Some(problem)),
    }
}

pub(super) async fn sync_with(env: &impl Env) {
    if !env.available().await || !env.permission_granted().await {
        return;
    }
    let Some(seen) = active_host(env).await else {
        return;
    };
    let _ops = PUSH_OPS.lock().await;
    let Some(host) = active_host(env)
        .await
        .filter(|current| same_host(current, &seen))
    else {
        return;
    };
    if host.push {
        refresh_with(env, &host).await;
    } else {
        setup_with(env, &host).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicBool,
        Ordering,
    };
    use std::sync::{
        Mutex,
        PoisonError,
    };
    use std::time::Duration;

    use super::*;
    use crate::push::stub::Stub;
    use crate::remote::test_host::{
        start_host_with_relay,
        Host,
    };

    struct TestEnv {
        relay: String,
        host: Mutex<Option<AlertHost>>,
        enabled: AtomicBool,
        problem: Mutex<Option<String>>,
    }

    impl TestEnv {
        fn new(relay: &str, host: AlertHost) -> Self {
            Self {
                relay: relay.to_owned(),
                host: Mutex::new(Some(host)),
                enabled: AtomicBool::new(true),
                problem: Mutex::new(None),
            }
        }

        fn current(&self) -> Option<AlertHost> {
            self.host
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        fn problem(&self) -> Option<String> {
            self.problem
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl Env for TestEnv {
        fn relay_url(&self) -> &str {
            &self.relay
        }

        async fn notifications_enabled(&self) -> bool {
            self.enabled.load(Ordering::SeqCst)
        }

        async fn stored_host(&self) -> Option<AlertHost> {
            self.current()
        }

        async fn available(&self) -> bool {
            true
        }

        async fn permission_granted(&self) -> bool {
            true
        }

        async fn device_token(&self) -> Result<String> {
            Ok("apns-token".to_owned())
        }

        async fn mark_push(&self, host: &AlertHost, push: bool) {
            if let Some(stored) = self
                .host
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_mut()
            {
                if same_host(stored, host) {
                    stored.push = push;
                }
            }
        }

        fn set_problem(&self, problem: Option<String>) {
            *self.problem.lock().unwrap_or_else(PoisonError::into_inner) = problem;
        }
    }

    async fn paired(relay: Option<&str>) -> (Host, AlertHost) {
        let host = start_host_with_relay(relay).await;
        let code = pipedash_web::pairing::create_pairing(&host.state, &host.url.url)
            .unwrap()
            .code;
        let alert_host = crate::remote::alerts::claim_alert_host(host.url.clone(), &code, true)
            .await
            .unwrap();

        (host, alert_host)
    }

    async fn relay_stub() -> Stub {
        Stub::start_with_body(
            |request| match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/v1/subscriptions") => {
                    (201, r#"{"subscription_id":"sub-new"}"#.to_owned())
                }
                _ => (204, String::new()),
            },
        )
        .await
    }

    fn seed_registration(device_id: &str, subscription_id: &str) {
        save_registration(&Registration {
            subscription_id: subscription_id.to_owned(),
            device_id: device_id.to_owned(),
        })
        .unwrap();
    }

    #[tokio::test]
    async fn a_host_that_dropped_its_relay_is_told_apart_from_one_that_still_pushes() {
        let relay = "https://relay.pipedash.app";
        let (_off, off_host) = paired(None).await;
        assert_eq!(
            host_push(&off_host, relay).await,
            Some(HostPush::RelayStopped)
        );

        let (_other, other_host) = paired(Some("https://relay.example.com")).await;
        assert_eq!(
            host_push(&other_host, relay).await,
            Some(HostPush::RelayStopped)
        );

        let (on, on_host) = paired(Some(&format!("{relay}/"))).await;
        assert_eq!(
            host_push(&on_host, relay).await,
            Some(HostPush::NoRegistration)
        );

        on.core
            .device_pushes()
            .set(&on_host.device_id, "sub-1")
            .await
            .unwrap();
        assert_eq!(
            host_push(&on_host, relay).await,
            Some(HostPush::Active {
                undelivered_after: None
            })
        );

        let unreachable = AlertHost {
            server_url: "http://127.0.0.1:1".to_owned(),
            ..on_host
        };
        assert_eq!(host_push(&unreachable, relay).await, None);
    }

    #[tokio::test]
    async fn a_sync_that_was_waiting_when_notifications_went_off_registers_nothing() {
        let relay = relay_stub().await;
        let (host, alert_host) = paired(Some(&relay.url)).await;
        let alert_host = AlertHost {
            push: true,
            ..alert_host
        };
        host.core
            .device_pushes()
            .set(&alert_host.device_id, "sub-old")
            .await
            .unwrap();
        seed_registration(&alert_host.device_id, "sub-old");
        let env = std::sync::Arc::new(TestEnv::new(&relay.url, alert_host.clone()));

        let held = PUSH_OPS.lock().await;
        let pending = tokio::spawn({
            let env = std::sync::Arc::clone(&env);
            async move { sync_with(&*env).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!pending.is_finished());

        env.enabled.store(false, Ordering::SeqCst);
        disable_with(&*env).await;
        *env.host.lock().unwrap_or_else(PoisonError::into_inner) = None;
        drop(held);
        pending.await.unwrap();

        assert_eq!(relay.count("PUT", "/v1/subscriptions/sub-old"), 0);
        assert_eq!(relay.count("POST", "/v1/subscriptions"), 0);
        assert_eq!(relay.count("DELETE", "/v1/subscriptions/sub-old"), 1);
        assert!(registration().unwrap().is_none());
        assert!(host
            .core
            .device_pushes()
            .get(&alert_host.device_id)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_sync_for_a_replaced_host_registers_nothing() {
        let relay = relay_stub().await;
        let (host, alert_host) = paired(Some(&relay.url)).await;
        let alert_host = AlertHost {
            push: true,
            ..alert_host
        };
        let env = std::sync::Arc::new(TestEnv::new(&relay.url, alert_host.clone()));

        let held = PUSH_OPS.lock().await;
        let pending = tokio::spawn({
            let env = std::sync::Arc::clone(&env);
            async move { sync_with(&*env).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        *env.host.lock().unwrap_or_else(PoisonError::into_inner) = Some(AlertHost {
            device_id: "other".to_owned(),
            server_url: "https://other.example.com".to_owned(),
            ..alert_host.clone()
        });
        drop(held);
        pending.await.unwrap();

        assert_eq!(relay.count("POST", "/v1/subscriptions"), 0);
        assert!(host
            .core
            .device_pushes()
            .get(&alert_host.device_id)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_host_that_now_offers_push_is_set_up_on_the_next_sync() {
        let relay = relay_stub().await;
        let (host, alert_host) = paired(Some(&relay.url)).await;
        saved::delete().unwrap();
        let env = TestEnv::new(&relay.url, alert_host.clone());
        env.set_problem(Some("old problem".to_owned()));

        sync_with(&env).await;

        assert!(env.current().unwrap().push);
        assert_eq!(env.problem(), None);
        assert_eq!(relay.count("POST", "/v1/subscriptions"), 1);
        let row = host
            .core
            .device_pushes()
            .get(&alert_host.device_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.relay_subscription_id, "sub-new");
        assert_eq!(registration().unwrap().unwrap().subscription_id, "sub-new");
    }

    #[tokio::test]
    async fn a_host_that_still_offers_no_push_is_left_alone() {
        let relay = relay_stub().await;
        let (_host, alert_host) = paired(None).await;
        let env = TestEnv::new(&relay.url, alert_host);

        sync_with(&env).await;

        assert!(!env.current().unwrap().push);
        assert_eq!(relay.count("POST", "/v1/subscriptions"), 0);
    }

    #[tokio::test]
    async fn a_host_that_stopped_its_relay_ends_push_and_explains_why() {
        let relay = relay_stub().await;
        let (_host, alert_host) = paired(None).await;
        seed_registration(&alert_host.device_id, "sub-old");
        let env = TestEnv::new(
            &relay.url,
            AlertHost {
                push: true,
                ..alert_host
            },
        );

        sync_with(&env).await;

        assert!(!env.current().unwrap().push);
        assert_eq!(
            env.problem().as_deref(),
            Some(
                "Studio stopped sending push alerts. Pipedash checks it in the background instead."
            )
        );
        assert_eq!(relay.count("DELETE", "/v1/subscriptions/sub-old"), 1);
        assert!(registration().unwrap().is_none());
    }
}
