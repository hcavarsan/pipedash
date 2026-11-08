use std::collections::HashMap;

use pipedash_plugin_api::config::{
    required_value,
    value,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    SignInClient,
    SignInToken,
};
use reqwest::Url;

use crate::config;

const GENERIC_LABEL: &str = "SSO";
const OPENID_SCOPE: &str = "openid";

fn split_scopes(raw: &str) -> Vec<String> {
    let mut scopes: Vec<String> = Vec::new();
    for scope in raw
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|scope| !scope.is_empty())
    {
        if !scopes.iter().any(|existing| existing == scope) {
            scopes.push(scope.to_string());
        }
    }
    scopes
}

fn scopes(raw: &str, token: SignInToken) -> Vec<String> {
    let mut scopes = split_scopes(raw);
    if scopes.is_empty() {
        scopes = split_scopes(config::DEFAULT_SCOPES);
    }
    if token == SignInToken::IdToken && !scopes.iter().any(|scope| scope == OPENID_SCOPE) {
        scopes.insert(0, OPENID_SCOPE.to_string());
    }
    scopes
}

fn authorize_params(raw: Option<&str>) -> PluginResult<Vec<(String, String)>> {
    raw.unwrap_or_default()
        .split('&')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            Some((key, value)) if !key.trim().is_empty() => {
                Ok((key.trim().to_string(), value.trim().to_string()))
            }
            _ => Err(PluginError::InvalidConfig(
                "Write extra sign-in parameters as key=value pairs joined by &".to_string(),
            )),
        })
        .collect()
}

pub(crate) fn client_for(config: &HashMap<String, String>) -> PluginResult<SignInClient> {
    let issuer = value(config, config::SSO_ISSUER)
        .map(|issuer| issuer.trim_end_matches('/').to_string())
        .filter(|issuer| !issuer.is_empty())
        .ok_or_else(|| {
            PluginError::InvalidConfig("Enter the identity provider URL to sign in".to_string())
        })?;
    let client_id = required_value(
        config,
        config::SSO_CLIENT_ID,
        "Enter the client ID registered for Pipedash",
    )?
    .to_string();
    let token = match value(config, config::SSO_TOKEN) {
        None | Some("id_token") => SignInToken::IdToken,
        Some("access_token") => SignInToken::AccessToken,
        Some(other) => {
            return Err(PluginError::InvalidConfig(format!(
                "Unknown token '{other}'. Use id_token or access_token"
            )))
        }
    };
    let redirect_uri =
        value(config, config::SSO_REDIRECT_URI).unwrap_or(config::DEFAULT_REDIRECT_URI);
    Url::parse(redirect_uri)
        .map_err(|e| PluginError::InvalidConfig(format!("Invalid redirect URI: {e}")))?;
    let label = Url::parse(&issuer)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| GENERIC_LABEL.to_string());

    Ok(SignInClient {
        label,
        scopes: scopes(
            value(config, config::SSO_SCOPES).unwrap_or(config::DEFAULT_SCOPES),
            token,
        ),
        issuer,
        client_id,
        redirect_uri: redirect_uri.to_string(),
        authorize_params: authorize_params(value(config, config::SSO_PARAMS))?,
        token,
    })
}

#[cfg(test)]
mod tests;
