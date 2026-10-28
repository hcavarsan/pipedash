use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
    Weak,
};
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::SignInClient;
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
use crate::infrastructure::oauth::loopback::{
    self,
    Loopback,
};
use crate::infrastructure::oauth::{
    self,
    Callback,
};

const PENDING_TTL: Duration = Duration::from_secs(10 * 60);
const COMPLETED_TTL: Duration = Duration::from_secs(30 * 60);
const FOREIGN_CALLBACK: &str =
    "This sign-in page belongs to another attempt. Start the sign-in again from Pipedash.";
const WRONG_ADDRESS: &str =
    "Paste the full address of the page your browser opened after you signed in.";
const MISSING_CODE: &str = "The sign-in server didn't send an authorization code.";
const TIMED_OUT: &str = "This sign-in timed out. Start it again from Pipedash.";
const UNFINISHED: &str = "Finish signing in before you continue.";

struct Pending {
    state: String,
    verifier: String,
    token_endpoint: Url,
    client: SignInClient,
}

struct Flow {
    status: watch::Sender<SignInStatus>,
    pending: Option<Pending>,
    credential: Option<SignInCredential>,
    http: Arc<reqwest::Client>,
    redirect_uri: Url,
    expires_at: Instant,
    listener: Option<JoinHandle<()>>,
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
        });
        flow.into_mut().listener = loopback
            .map(|loopback| tokio::spawn(listen(Arc::downgrade(self), id, loopback, expires_at)));
        Ok(start)
    }

    pub(crate) fn status(&self, id: &str) -> SignInStatus {
        self.prune();
        self.flows()
            .get(id)
            .map_or(SignInStatus::Expired, |flow| flow.status.borrow().clone())
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
            if flows.get(id).is_some_and(|flow| flow.credential.is_some()) {
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
        let (current, http) = self.completed(id)?;
        if !current.renew_at().is_some_and(|at| at <= Utc::now()) {
            return Ok(current);
        }

        let renewed = oauth::refresh(&http, &current).await?;
        if let Some(flow) = self.flows().get_mut(id) {
            flow.credential = Some(renewed.clone());
        }
        Ok(renewed)
    }

    fn completed(&self, id: &str) -> DomainResult<(SignInCredential, Arc<reqwest::Client>)> {
        self.prune();
        let flows = self.flows();
        let Some(flow) = flows.get(id) else {
            return Err(DomainError::ProviderAuthenticationFailed(
                SIGN_IN_EXPIRED.into(),
            ));
        };
        if let Some(credential) = &flow.credential {
            return Ok((credential.clone(), Arc::clone(&flow.http)));
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

    fn expire(&self, id: &str) {
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

fn failure_message(error: DomainError) -> String {
    match error {
        DomainError::ProviderAuthenticationFailed(message) => message,
        other => other.to_string(),
    }
}

async fn listen(service: Weak<SignInService>, id: String, loopback: Loopback, deadline: Instant) {
    loop {
        let (stream, url) = match tokio::time::timeout_at(deadline, loopback.accept()).await {
            Err(_) => {
                if let Some(service) = service.upgrade() {
                    service.expire(&id);
                }
                return;
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "Stopped listening for the sign-in redirect");
                return;
            }
            Ok(Ok(accepted)) => accepted,
        };
        let Some(service) = service.upgrade() else {
            return;
        };
        let outcome = service.finish(&id, Callback::from_url(&url)).await;
        let page = match &outcome {
            Ok(SignInStatus::Completed { .. }) => Ok(()),
            Ok(SignInStatus::Failed { message }) => Err(message.clone()),
            Ok(SignInStatus::Pending | SignInStatus::Expired) => Err(TIMED_OUT.to_string()),
            Err(error) => Err(failure_message(error.clone())),
        };
        loopback::respond(stream, &page).await;
        if outcome.is_ok() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use pipedash_plugin_api::SignInToken;
    use serde_json::json;

    use super::*;
    use crate::infrastructure::oauth::loopback::tests::{
        free_redirect_uri,
        get,
    };
    use crate::infrastructure::oauth::test_idp::{
        jwt,
        Idp,
    };

    async fn setup() -> (Arc<SignInService>, Idp, SignInClient) {
        let idp = Idp::start().await;
        idp.reply(
            200,
            json!({"access_token": "a", "id_token": jwt(json!({"email": "dev@example.com"})), "refresh_token": "r"}),
        );
        let client = SignInClient {
            label: "Dex".into(),
            issuer: idp.base_url.clone(),
            client_id: "cli".into(),
            scopes: vec!["openid".into()],
            redirect_uri: free_redirect_uri().await.to_string(),
            authorize_params: Vec::new(),
            token: SignInToken::IdToken,
        };
        (Arc::new(SignInService::default()), idp, client)
    }

    fn callback(client: &SignInClient, start: &SignInStart, query: &str) -> Url {
        let state = Url::parse(&start.authorize_url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        let query = query.replace("{state}", &state);
        Url::parse(&format!("{}?{query}", client.redirect_uri)).unwrap()
    }

    #[tokio::test]
    async fn browser_redirect_completes_the_flow() {
        let (service, _idp, client) = setup().await;
        let start = service
            .start(client.clone(), Arc::new(reqwest::Client::new()))
            .await
            .unwrap();
        assert!(!start.manual);

        let (status, body) = get(&callback(&client, &start, "code=c&state={state}")).await;

        assert_eq!(status, 200);
        assert!(body.contains("Signed in"));
        let completed = SignInStatus::Completed {
            account: Some("dev@example.com".into()),
        };
        assert_eq!(service.wait(&start.id).await, completed);
        assert_eq!(
            service
                .credential(&start.id)
                .await
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("r")
        );
    }

    #[tokio::test]
    async fn foreign_state_keeps_waiting_and_provider_errors_fail() {
        let (service, _idp, client) = setup().await;
        let start = service
            .start(client.clone(), Arc::new(reqwest::Client::new()))
            .await
            .unwrap();

        let (status, _) = get(&callback(&client, &start, "code=c&state=other")).await;
        assert_eq!(status, 400);
        assert_eq!(service.status(&start.id), SignInStatus::Pending);

        let (status, body) = get(&callback(
            &client,
            &start,
            "error=access_denied&error_description=Denied&state={state}",
        ))
        .await;
        assert_eq!(status, 400);
        assert!(body.contains("Denied"));
        assert_eq!(
            service.status(&start.id),
            SignInStatus::Failed {
                message: "Denied".into()
            }
        );
    }

    #[tokio::test]
    async fn pasted_address_completes_only_for_the_redirect() {
        let (service, _idp, client) = setup().await;
        let start = service
            .start(client.clone(), Arc::new(reqwest::Client::new()))
            .await
            .unwrap();

        let foreign = service
            .complete(&start.id, "https://example.com/auth/callback?code=c")
            .await;
        assert!(matches!(foreign, Err(DomainError::InvalidConfig(m)) if m == WRONG_ADDRESS));

        let pasted = callback(&client, &start, "code=c&state={state}");
        let status = service
            .complete(&start.id, &format!("  {pasted} "))
            .await
            .unwrap();
        assert!(matches!(status, SignInStatus::Completed { .. }));
    }

    #[tokio::test]
    async fn cancel_frees_the_redirect_port_and_expires_the_flow() {
        let (service, _idp, client) = setup().await;
        let http = Arc::new(reqwest::Client::new());
        let first = service.start(client.clone(), http.clone()).await.unwrap();

        service.cancel(&first.id).await;

        assert_eq!(service.status(&first.id), SignInStatus::Expired);
        assert!(service.credential(&first.id).await.is_err());
        let second = service.start(client, http).await.unwrap();
        assert!(!second.manual);
    }

    #[tokio::test]
    async fn cancel_keeps_a_completed_sign_in_for_saving() {
        let (service, _idp, client) = setup().await;
        let start = service
            .start(client.clone(), Arc::new(reqwest::Client::new()))
            .await
            .unwrap();
        let pasted = callback(&client, &start, "code=c&state={state}");
        service.complete(&start.id, pasted.as_str()).await.unwrap();

        service.cancel(&start.id).await;

        assert!(matches!(
            service.status(&start.id),
            SignInStatus::Completed { .. }
        ));
        assert!(service.credential(&start.id).await.is_ok());
    }

    #[tokio::test]
    async fn a_completed_sign_in_is_renewed_once_before_it_is_used() {
        let (service, idp, client) = setup().await;
        let expired = jwt(json!({"email": "dev@example.com", "exp": Utc::now().timestamp() - 10}));
        idp.reply(
            200,
            json!({"access_token": "a", "id_token": expired, "refresh_token": "r1"}),
        );
        let start = service
            .start(client.clone(), Arc::new(reqwest::Client::new()))
            .await
            .unwrap();
        let pasted = callback(&client, &start, "code=c&state={state}");
        service.complete(&start.id, pasted.as_str()).await.unwrap();
        let fresh = jwt(json!({"email": "dev@example.com", "exp": Utc::now().timestamp() + 3600}));
        idp.reply(
            200,
            json!({"access_token": "a", "id_token": fresh, "refresh_token": "r2"}),
        );

        let first = service.credential(&start.id).await.unwrap();
        let second = service.credential(&start.id).await.unwrap();

        assert_eq!(first.token, fresh);
        assert_eq!(first.refresh_token.as_deref(), Some("r2"));
        assert_eq!(second, first);
        assert_eq!(idp.forms.lock().unwrap().len(), 2);
    }
}
