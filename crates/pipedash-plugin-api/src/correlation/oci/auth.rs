use reqwest::header::{
    HeaderMap,
    ACCEPT,
    WWW_AUTHENTICATE,
};
use reqwest::{
    Response,
    StatusCode,
    Url,
};
use serde::Deserialize;

use super::authority::strip_default_port;
use super::{
    is_loopback_host,
    OciClient,
};
use crate::correlation::hooks::RegistryAuth;
use crate::error::{
    PluginError,
    PluginResult,
};
use crate::http::{
    check_response,
    decode_json,
    redact_url,
    send,
    with_query,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Challenge {
    Bearer {
        realm: String,
        service: Option<String>,
        scope: Option<String>,
    },
    Basic,
}

pub(super) enum Grant {
    Bearer(String),
    Basic(String, String),
}

#[derive(Deserialize)]
struct TokenResponse {
    token: Option<String>,
    access_token: Option<String>,
}

pub(super) fn parse_challenge(headers: &HeaderMap) -> Option<Challenge> {
    headers
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(parse_header)
        .min_by_key(|challenge| matches!(challenge, Challenge::Basic))
}

fn parse_header(value: &str) -> Option<Challenge> {
    let value = value.trim();
    let (scheme, rest) = value.split_once(char::is_whitespace).unwrap_or((value, ""));
    if scheme.eq_ignore_ascii_case("basic") {
        return Some(Challenge::Basic);
    }
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let params = parse_params(rest);
    let find = |key: &str| {
        params
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };
    Some(Challenge::Bearer {
        realm: find("realm")?,
        service: find("service"),
        scope: find("scope"),
    })
}

fn parse_params(mut rest: &str) -> Vec<(String, String)> {
    let mut params = Vec::new();
    rest = rest.trim_start_matches([',', ' ']);
    while let Some((key, after)) = rest.split_once('=') {
        let after = after.trim_start();
        let (value, tail) = match after.strip_prefix('"') {
            Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
            None => after.split_once(',').unwrap_or((after, "")),
        };
        params.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        rest = tail.trim_start_matches([',', ' ']);
    }
    params
}

pub(super) fn realm_url(
    realm: &str, service: Option<String>, scope: Option<String>,
) -> PluginResult<Url> {
    let url = Url::parse(realm)
        .map_err(|error| PluginError::ApiError(format!("Invalid registry token realm: {error}")))?;
    let secure = url.scheme() == "https"
        || (url.scheme() == "http" && url.host_str().is_some_and(is_loopback_host));
    if !secure || !url.username().is_empty() || url.password().is_some() {
        return Err(PluginError::ApiError(format!(
            "Refusing registry token realm {}",
            redact_url(&url)
        )));
    }
    let query: Vec<(&str, String)> = [("service", service), ("scope", scope)]
        .into_iter()
        .filter_map(|(key, value)| Some((key, value?)))
        .collect();
    Ok(with_query(url, &query))
}

fn authority(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

pub(super) fn may_receive_secret(auth: &RegistryAuth, registry: &Url, realm: &Url) -> bool {
    let target = authority(realm);
    target == authority(registry)
        || auth
            .token_hosts
            .iter()
            .any(|host| strip_default_port(host.trim(), realm.scheme()) == target)
}

fn challenge_of(response: &Response, auth: Option<&RegistryAuth>) -> Option<Challenge> {
    let answerable =
        response.status() == StatusCode::UNAUTHORIZED && !auth.is_some_and(|auth| auth.bearer);
    answerable
        .then(|| parse_challenge(response.headers()))
        .flatten()
}

impl OciClient {
    pub(super) async fn request(
        &self, url: &Url, accept: Option<&str>, grant: Option<&Grant>,
    ) -> PluginResult<Response> {
        let mut request = self.http.get(url.clone());
        if let Some(accept) = accept {
            request = request.header(ACCEPT, accept);
        }
        request = match grant {
            Some(Grant::Bearer(token)) => request.bearer_auth(token),
            Some(Grant::Basic(username, secret)) => request.basic_auth(username, Some(secret)),
            None => request,
        };
        send(url, request).await
    }

    pub(super) async fn probe(
        &self, url: &Url, accept: Option<&str>, auth: Option<&RegistryAuth>,
    ) -> PluginResult<Response> {
        let grant = auth
            .filter(|auth| auth.bearer)
            .map(|auth| Grant::Bearer(auth.secret.clone()));
        self.request(url, accept, grant.as_ref()).await
    }

    pub(super) async fn authed_get(
        &self, url: &Url, accept: Option<&str>, auth: Option<&RegistryAuth>,
    ) -> PluginResult<Response> {
        let first = self.probe(url, accept, auth).await?;
        match self.answer(url, &first, auth).await? {
            Some(grant) => self.request(url, accept, Some(&grant)).await,
            None => Ok(first),
        }
    }

    pub(super) async fn answer(
        &self, url: &Url, first: &Response, auth: Option<&RegistryAuth>,
    ) -> PluginResult<Option<Grant>> {
        match challenge_of(first, auth) {
            Some(challenge) => self.authorize(url, challenge, auth).await,
            None => Ok(None),
        }
    }

    async fn authorize(
        &self, url: &Url, challenge: Challenge, auth: Option<&RegistryAuth>,
    ) -> PluginResult<Option<Grant>> {
        match challenge {
            Challenge::Basic => Ok(auth
                .and_then(|auth| Some(Grant::Basic(auth.username.clone()?, auth.secret.clone())))),
            Challenge::Bearer {
                realm,
                service,
                scope,
            } => {
                let token = self.exchange(url, &realm, service, scope, auth).await?;
                Ok(Some(Grant::Bearer(token)))
            }
        }
    }

    async fn exchange(
        &self, registry: &Url, realm: &str, service: Option<String>, scope: Option<String>,
        auth: Option<&RegistryAuth>,
    ) -> PluginResult<String> {
        let url = realm_url(realm, service, scope)?;
        let mut request = self.http.get(url.clone());
        if let Some(RegistryAuth {
            username: Some(username),
            secret,
            ..
        }) = auth.filter(|auth| may_receive_secret(auth, registry, &url))
        {
            request = request.basic_auth(username, Some(secret));
        }
        let response = check_response(send(&url, request).await?).await?;
        let body: TokenResponse = decode_json(response, "registry token").await?;
        body.token
            .or(body.access_token)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                PluginError::AuthenticationFailed("Registry token response had no token".into())
            })
    }
}
