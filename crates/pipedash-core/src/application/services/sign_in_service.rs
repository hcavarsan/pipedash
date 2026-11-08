use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::{
    SignInClient,
    SignInRefreshRejection,
};
use reqwest::Url;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::domain::{
    DomainError,
    DomainResult,
    SignInCredential,
    SignInStart,
    SignInStatus,
    SIGN_IN_EXPIRED,
};
use crate::infrastructure::oauth::loopback::Loopback;
use crate::infrastructure::oauth::{
    self,
    Callback,
};

mod listener;

const PENDING_TTL: Duration = Duration::from_secs(10 * 60);
const COMPLETED_TTL: Duration = Duration::from_secs(30 * 60);
const FOREIGN_CALLBACK: &str =
    "This sign-in page belongs to another attempt. Start the sign-in again from Pipedash.";
const WRONG_ADDRESS: &str =
    "Paste the full address of the page your browser opened after you signed in.";
const MISSING_CODE: &str = "The sign-in server didn't send an authorization code.";
const UNFINISHED: &str = "Finish signing in before you continue.";

struct Pending {
    state: String,
    verifier: String,
    token_endpoint: Url,
    client: SignInClient,
}

#[derive(Clone)]
pub(crate) struct ProviderSignIn {
    pub id: i64,
    pub provider_type: String,
    pub token: String,
    pub client: SignInClient,
}

struct Flow {
    status: watch::Sender<SignInStatus>,
    pending: Option<Pending>,
    credential: Option<SignInCredential>,
    http: Arc<reqwest::Client>,
    redirect_uri: Url,
    expires_at: Instant,
    listener: Option<JoinHandle<()>>,
    rejections: &'static [SignInRefreshRejection],
    owner: Option<ProviderSignIn>,
}

impl Flow {
    fn close(&mut self) -> Option<JoinHandle<()>> {
        self.status.send_replace(SignInStatus::Expired);
        let listener = self.listener.take();
        if let Some(listener) = &listener {
            listener.abort();
        }
        listener
    }
}

#[derive(Default)]
pub(crate) struct SignInService {
    flows: Mutex<HashMap<String, Flow>>,
    renewal: tokio::sync::Mutex<()>,
}

impl SignInService {
    fn flows(&self) -> MutexGuard<'_, HashMap<String, Flow>> {
        self.flows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn prune(&self) {
        let now = Instant::now();
        self.flows().retain(|_, flow| {
            let alive = flow.expires_at > now;
            if !alive {
                flow.close();
            }
            alive
        });
    }

    pub(crate) async fn start(
        self: &Arc<Self>, client: SignInClient, http: Arc<reqwest::Client>,
        rejections: &'static [SignInRefreshRejection], owner: Option<ProviderSignIn>,
    ) -> DomainResult<SignInStart> {
        self.prune();
        let endpoints = oauth::discover(&http, &client.issuer).await?;
        let authorization = oauth::authorize(&client, &endpoints);
        let redirect_uri = Url::parse(&client.redirect_uri).map_err(|e| {
            DomainError::InvalidConfig(format!(
                "Invalid sign-in redirect {}: {e}",
                client.redirect_uri
            ))
        })?;
        let loopback = Loopback::bind(&redirect_uri).await;
        let id = oauth::random_id::<16>();
        let expires_at = Instant::now() + PENDING_TTL;
        let start = SignInStart {
            id: id.clone(),
            authorize_url: authorization.url.to_string(),
            label: client.label.clone(),
            manual: loopback.is_none(),
        };

        let mut flows = self.flows();
        let flow = flows.entry(id.clone()).insert_entry(Flow {
            status: watch::channel(SignInStatus::Pending).0,
            pending: Some(Pending {
                state: authorization.state,
                verifier: authorization.verifier,
                token_endpoint: endpoints.token,
                client,
            }),
            credential: None,
            http,
            redirect_uri,
            expires_at,
            listener: None,
            rejections,
            owner,
        });
        flow.into_mut().listener = loopback.map(|loopback| {
            tokio::spawn(listener::listen(
                Arc::downgrade(self),
                id,
                loopback,
                expires_at,
            ))
        });
        Ok(start)
    }

    pub(crate) fn status(&self, id: &str) -> SignInStatus {
        self.prune();
        self.flows()
            .get(id)
            .map_or(SignInStatus::Expired, |flow| flow.status.borrow().clone())
    }

    pub(crate) fn provider_flow(&self, id: &str, provider_id: i64) -> DomainResult<ProviderSignIn> {
        self.prune();
        self.flows()
            .get(id)
            .and_then(|flow| flow.owner.as_ref())
            .filter(|owner| owner.id == provider_id)
            .cloned()
            .ok_or_else(|| {
                DomainError::InvalidConfig(
                    "This sign-in belongs to another provider or expired".into(),
                )
            })
    }

    pub(crate) fn check_owner(&self, id: &str, provider_id: Option<i64>) -> DomainResult<()> {
        self.prune();
        let flows = self.flows();
        let flow = flows
            .get(id)
            .ok_or_else(|| DomainError::ProviderAuthenticationFailed(SIGN_IN_EXPIRED.into()))?;
        if flow
            .owner
            .as_ref()
            .is_some_and(|owner| Some(owner.id) != provider_id)
        {
            return Err(DomainError::InvalidConfig(
                "This sign-in belongs to another provider".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn wait(&self, id: &str) -> SignInStatus {
        self.prune();
        let watched = self
            .flows()
            .get(id)
            .map(|flow| (flow.status.subscribe(), flow.expires_at));
        let Some((mut status, deadline)) = watched else {
            return SignInStatus::Expired;
        };
        let settled = status.wait_for(|s| *s != SignInStatus::Pending);
        let outcome = match tokio::time::timeout_at(deadline, settled).await {
            Ok(Ok(settled)) => settled.clone(),
            _ => SignInStatus::Expired,
        };
        outcome
    }

    pub(crate) async fn finish(&self, id: &str, callback: Callback) -> DomainResult<SignInStatus> {
        let (pending, http) = {
            let mut flows = self.flows();
            let Some(flow) = flows.get_mut(id) else {
                return Ok(SignInStatus::Expired);
            };
            let matching = flow
                .pending
                .take_if(|pending| callback.state.as_deref() == Some(pending.state.as_str()));
            match matching {
                Some(pending) => (pending, Arc::clone(&flow.http)),
                None if flow.pending.is_some() => {
                    return Err(DomainError::InvalidConfig(FOREIGN_CALLBACK.into()));
                }
                None => return Ok(flow.status.borrow().clone()),
            }
        };

        let Callback {
            code,
            error,
            error_description,
            ..
        } = callback;
        let result = match (error, code) {
            (Some(error), _) => Err(error_description.unwrap_or(error)),
            (None, None) => Err(MISSING_CODE.to_string()),
            (None, Some(code)) => oauth::exchange(
                &http,
                &pending.token_endpoint,
                &pending.client,
                &code,
                &pending.verifier,
            )
            .await
            .map_err(failure_message),
        };

        let mut flows = self.flows();
        let Some(flow) = flows.get_mut(id) else {
            return Ok(SignInStatus::Expired);
        };
        let status = match result {
            Ok(credential) => {
                let account = credential.account.clone();
                flow.credential = Some(credential);
                flow.expires_at = Instant::now() + COMPLETED_TTL;
                SignInStatus::Completed { account }
            }
            Err(message) => SignInStatus::Failed { message },
        };
        flow.status.send_replace(status.clone());
        Ok(status)
    }

    pub(crate) async fn complete(
        &self, id: &str, redirect_url: &str,
    ) -> DomainResult<SignInStatus> {
        self.prune();
        let redirect_uri = {
            let flows = self.flows();
            let Some(flow) = flows.get(id) else {
                return Ok(SignInStatus::Expired);
            };
            if flow.pending.is_none() {
                return Ok(flow.status.borrow().clone());
            }
            flow.redirect_uri.clone()
        };
        let pasted = Url::parse(redirect_url.trim())
            .ok()
            .filter(|url| {
                url.scheme() == redirect_uri.scheme()
                    && url.host_str() == redirect_uri.host_str()
                    && url.port_or_known_default() == redirect_uri.port_or_known_default()
                    && url.path() == redirect_uri.path()
            })
            .ok_or_else(|| DomainError::InvalidConfig(WRONG_ADDRESS.into()))?;

        let status = self.finish(id, Callback::from_url(&pasted)).await?;
        if status != SignInStatus::Pending {
            let listener = self
                .flows()
                .get_mut(id)
                .and_then(|flow| flow.listener.take());
            if let Some(listener) = listener {
                listener.abort();
                let _ = listener.await;
            }
        }
        Ok(status)
    }

    pub(crate) async fn cancel(&self, id: &str) {
        let listener = {
            let mut flows = self.flows();
            if flows
                .get(id)
                .is_some_and(|flow| flow.credential.is_some() && flow.owner.is_none())
            {
                return;
            }
            flows.remove(id).and_then(|mut flow| flow.close())
        };
        if let Some(listener) = listener {
            let _ = listener.await;
        }
    }

    pub(crate) async fn credential(&self, id: &str) -> DomainResult<SignInCredential> {
        let _renewal = self.renewal.lock().await;
        let (current, http, rejections) = self.completed(id)?;
        if !current.renew_at().is_some_and(|at| at <= Utc::now()) {
            return Ok(current);
        }

        let renewed = oauth::refresh(&http, &current, rejections).await?;
        if let Some(flow) = self.flows().get_mut(id) {
            flow.credential = Some(renewed.clone());
        }
        Ok(renewed)
    }

    fn completed(
        &self, id: &str,
    ) -> DomainResult<(
        SignInCredential,
        Arc<reqwest::Client>,
        &'static [SignInRefreshRejection],
    )> {
        self.prune();
        let flows = self.flows();
        let Some(flow) = flows.get(id) else {
            return Err(DomainError::ProviderAuthenticationFailed(
                SIGN_IN_EXPIRED.into(),
            ));
        };
        if let Some(credential) = &flow.credential {
            return Ok((credential.clone(), Arc::clone(&flow.http), flow.rejections));
        }
        let status = flow.status.borrow().clone();
        Err(match status {
            SignInStatus::Pending => DomainError::InvalidConfig(UNFINISHED.into()),
            SignInStatus::Failed { message } => DomainError::ProviderAuthenticationFailed(message),
            SignInStatus::Completed { .. } | SignInStatus::Expired => {
                DomainError::ProviderAuthenticationFailed(SIGN_IN_EXPIRED.into())
            }
        })
    }

    pub(crate) fn forget(&self, id: &str) {
        self.flows().remove(id);
    }

    pub(super) fn expire(&self, id: &str) {
        if let Some(mut flow) = self.flows().remove(id) {
            flow.close();
        }
    }
}

impl Drop for SignInService {
    fn drop(&mut self) {
        for flow in self.flows().values_mut() {
            flow.close();
        }
    }
}

pub(super) fn failure_message(error: DomainError) -> String {
    match error {
        DomainError::ProviderAuthenticationFailed(message) => message,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests;
