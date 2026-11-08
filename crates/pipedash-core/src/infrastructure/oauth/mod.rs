pub(crate) mod loopback;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{
    DateTime,
    Duration,
    Utc,
};
use pipedash_plugin_api::{
    from_status,
    map_send_error,
    redact_url,
    truncate_chars,
    SignInClient,
    SignInRefreshRejection,
    SignInToken,
};
use reqwest::Url;
use serde::Deserialize;
use serde_json::{
    Map,
    Value,
};
use sha2::{
    Digest,
    Sha256,
};

use crate::domain::{
    DomainError,
    DomainResult,
    SignInCredential,
    SIGN_IN_EXPIRED,
};
use crate::infrastructure::providers::PluginAdapter;

const OFFLINE_ACCESS: &str = "offline_access";
const ACCOUNT_CLAIMS: [&str; 4] = ["email", "preferred_username", "name", "sub"];
const EXPIRED_GRANT_ERRORS: [&str; 3] = ["invalid_grant", "invalid_client", "unauthorized_client"];
const BODY_PREVIEW_CHARS: usize = 300;

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    pub authorization: Url,
    pub token: Url,
    pub scopes_supported: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Discovery {
    authorization_endpoint: Url,
    token_endpoint: Url,
    #[serde(default)]
    scopes_supported: Option<Vec<String>>,
}

pub(crate) struct Authorization {
    pub url: Url,
    pub state: String,
    pub verifier: String,
}

#[derive(Debug, Default)]
pub(crate) struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

impl Callback {
    pub(crate) fn from_url(url: &Url) -> Self {
        let mut callback = Self::default();
        for (key, value) in url.query_pairs() {
            let slot = match key.as_ref() {
                "code" => &mut callback.code,
                "state" => &mut callback.state,
                "error" => &mut callback.error,
                "error_description" => &mut callback.error_description,
                _ => continue,
            };
            *slot = Some(value.into_owned());
        }
        callback
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Clone, Copy)]
enum Grant {
    Exchange,
    Refresh,
}

pub(crate) fn random_id<const N: usize>() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; N]>())
}

fn parse_url(raw: &str) -> DomainResult<Url> {
    Url::parse(raw)
        .map_err(|e| DomainError::InvalidConfig(format!("Invalid sign-in URL {raw}: {e}")))
}

fn send_error(err: reqwest::Error, url: &Url) -> DomainError {
    PluginAdapter::map_error(map_send_error(err, url))
}

fn web_endpoint(url: Url) -> DomainResult<Url> {
    if matches!(url.scheme(), "http" | "https") {
        Ok(url)
    } else {
        Err(DomainError::InvalidConfig(format!(
            "The sign-in server returned an address that isn't a web page: {}",
            redact_url(&url)
        )))
    }
}

pub(crate) async fn discover(http: &reqwest::Client, issuer: &str) -> DomainResult<Endpoints> {
    let url = parse_url(&format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    ))?;
    let response = http
        .get(url.clone())
        .send()
        .await
        .map_err(|e| send_error(e, &url))?;
    let status = response.status();
    if !status.is_success() {
        return Err(DomainError::InvalidConfig(format!(
            "Couldn't read the sign-in settings from {} ({status})",
            redact_url(&url)
        )));
    }
    let discovery: Discovery = response.json().await.map_err(|e| {
        DomainError::ApiError(format!(
            "The sign-in server at {} returned invalid settings: {}",
            redact_url(&url),
            e.without_url()
        ))
    })?;
    Ok(Endpoints {
        authorization: web_endpoint(discovery.authorization_endpoint)?,
        token: web_endpoint(discovery.token_endpoint)?,
        scopes_supported: discovery.scopes_supported,
    })
}

pub(crate) fn authorize(client: &SignInClient, endpoints: &Endpoints) -> Authorization {
    let verifier = random_id::<32>();
    let state = random_id::<32>();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

    let mut scopes = client.scopes.clone();
    let offline_supported = endpoints
        .scopes_supported
        .as_ref()
        .is_none_or(|supported| supported.iter().any(|scope| scope == OFFLINE_ACCESS));
    if offline_supported && !scopes.iter().any(|scope| scope == OFFLINE_ACCESS) {
        scopes.push(OFFLINE_ACCESS.to_string());
    }

    let mut url = endpoints.authorization.clone();
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client.client_id)
        .append_pair("redirect_uri", &client.redirect_uri)
        .append_pair("scope", &scopes.join(" "))
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .extend_pairs(
            client
                .authorize_params
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );

    Authorization {
        url,
        state,
        verifier,
    }
}

pub(crate) async fn exchange(
    http: &reqwest::Client, token_endpoint: &Url, client: &SignInClient, code: &str, verifier: &str,
) -> DomainResult<SignInCredential> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", client.redirect_uri.as_str()),
        ("client_id", client.client_id.as_str()),
        ("code_verifier", verifier),
    ];
    let response = request_token(http, token_endpoint, &form, Grant::Exchange, &[]).await?;
    credential(
        response,
        client.token,
        token_endpoint,
        &client.client_id,
        None,
    )
}

pub(crate) async fn refresh(
    http: &reqwest::Client, current: &SignInCredential, rejections: &[SignInRefreshRejection],
) -> DomainResult<SignInCredential> {
    let refresh_token = current
        .refresh_token
        .as_deref()
        .ok_or_else(|| DomainError::ProviderAuthenticationFailed(SIGN_IN_EXPIRED.into()))?;
    let token_endpoint = parse_url(&current.token_endpoint)?;
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", current.client_id.as_str()),
    ];
    let response = request_token(http, &token_endpoint, &form, Grant::Refresh, rejections).await?;
    credential(
        response,
        current.kind,
        &token_endpoint,
        &current.client_id,
        Some(current),
    )
}

async fn request_token(
    http: &reqwest::Client, endpoint: &Url, form: &[(&str, &str)], grant: Grant,
    rejections: &[SignInRefreshRejection],
) -> DomainResult<TokenResponse> {
    let response = http
        .post(endpoint.clone())
        .form(form)
        .send()
        .await
        .map_err(|e| send_error(e, endpoint))?;
    let status = response.status();
    let body = response.text().await.map_err(|e| send_error(e, endpoint))?;

    if status.is_success() {
        return serde_json::from_str(&body).map_err(|e| {
            DomainError::ApiError(format!(
                "The sign-in server returned an invalid token response: {e}"
            ))
        });
    }

    if let Ok(error) = serde_json::from_str::<OAuthError>(&body) {
        match grant {
            Grant::Refresh
                if EXPIRED_GRANT_ERRORS.contains(&error.error.as_str())
                    || rejections.iter().any(|rejection| {
                        rejection.status == status.as_u16()
                            && rejection.error == error.error
                            && Some(rejection.description) == error.error_description.as_deref()
                    }) =>
            {
                return Err(DomainError::ProviderAuthenticationFailed(
                    SIGN_IN_EXPIRED.into(),
                ));
            }
            Grant::Exchange => {
                return Err(DomainError::ProviderAuthenticationFailed(
                    error.error_description.unwrap_or(error.error),
                ));
            }
            Grant::Refresh => {}
        }
    }

    let message = format!(
        "{} from {}: {}",
        status.as_u16(),
        redact_url(endpoint),
        truncate_chars(body.trim(), BODY_PREVIEW_CHARS)
    );
    Err(PluginAdapter::map_error(from_status(
        status.as_u16(),
        message,
        None,
    )))
}

fn credential(
    response: TokenResponse, kind: SignInToken, token_endpoint: &Url, client_id: &str,
    previous: Option<&SignInCredential>,
) -> DomainResult<SignInCredential> {
    let token = match kind {
        SignInToken::IdToken => response.id_token.clone().ok_or_else(|| {
            DomainError::ProviderAuthenticationFailed(
                "The sign-in server didn't return an ID token".into(),
            )
        })?,
        SignInToken::AccessToken => response.access_token,
    };
    let expires_at = jwt_claims(&token)
        .and_then(|claims| claims.get("exp")?.as_i64())
        .and_then(|exp| DateTime::from_timestamp(exp, 0))
        .or_else(|| {
            response
                .expires_in
                .map(|seconds| Utc::now() + Duration::seconds(seconds))
        });
    let account = response
        .id_token
        .as_deref()
        .and_then(jwt_claims)
        .and_then(|claims| account_of(&claims))
        .or_else(|| previous.and_then(|p| p.account.clone()));
    let refresh_token = response
        .refresh_token
        .or_else(|| previous.and_then(|p| p.refresh_token.clone()));

    Ok(SignInCredential {
        token,
        refresh_token,
        expires_at,
        token_endpoint: token_endpoint.to_string(),
        client_id: client_id.to_string(),
        kind,
        account,
    })
}

fn jwt_claims(token: &str) -> Option<Map<String, Value>> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn account_of(claims: &Map<String, Value>) -> Option<String> {
    ACCOUNT_CLAIMS.iter().find_map(|key| {
        claims
            .get(*key)?
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

#[cfg(test)]
pub(crate) mod test_idp;

#[cfg(test)]
mod tests;
