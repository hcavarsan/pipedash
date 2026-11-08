pub(super) mod platform;
mod registration;

use serde::Deserialize;

use self::platform::Env;
use self::registration::{
    delete_registration,
    registration,
    save_registration,
    Registration,
};
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
use crate::local_notifications::store::AlertHost;
use crate::remote::alerts::host_request;

pub(super) static PUSH_OPS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    let saved = match accepted {
        Ok(()) => save_registration(Registration {
            subscription_id: subscription_id.clone(),
            device_id: host.device_id.clone(),
        })
        .await
        .map_err(failed),
        Err(error) => Err(error),
    };
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
    let saved = registration().await.ok().flatten();
    register(env.relay_url(), &host, &device_token, saved)
        .await
        .map_err(|error| error.problem(&host))?;
    env.mark_push(&host, true).await;

    Ok(())
}

pub(super) async fn forget_at(relay_url: &str) {
    match registration().await {
        Ok(Some(saved)) => revoke_relay(relay_url, &saved.subscription_id).await,
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "could not read the push registration"),
    }
    if let Err(error) = delete_registration().await {
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
    let saved = registration().await.ok().flatten();
    match register(env.relay_url(), host, &device_token, saved).await {
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
mod tests;
