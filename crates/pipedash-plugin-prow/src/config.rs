use std::collections::HashMap;

use base64::Engine;
use pipedash_plugin_api::config::{
    comma_separated,
    value,
};
use pipedash_plugin_api::http::{
    parse_http_url,
    sensitive_header,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use reqwest::header::{
    HeaderName,
    HeaderValue,
    AUTHORIZATION,
    COOKIE,
};
use reqwest::Url;

pub(crate) const DEFAULT_DECK_URL: &str = "https://prow.k8s.io";
pub(crate) const DEFAULT_REDIRECT_URI: &str = "http://localhost:8085/auth/callback";
pub(crate) const DEFAULT_SCOPES: &str = "openid email profile";

pub(crate) const DECK_URL: &str = "deck_url";
pub(crate) const INSECURE: &str = "insecure";
pub(crate) const AUTH_SCHEME: &str = "auth_scheme";
pub(crate) const TOKEN: &str = "token";
pub(crate) const SSO_ISSUER: &str = "sso_issuer";
pub(crate) const SSO_CLIENT_ID: &str = "sso_client_id";
pub(crate) const SSO_SCOPES: &str = "sso_scopes";
pub(crate) const SSO_TOKEN: &str = "sso_token";
pub(crate) const SSO_REDIRECT_URI: &str = "sso_redirect_uri";
pub(crate) const SSO_PARAMS: &str = "sso_params";
pub(crate) const HISTORY_SOURCE: &str = "history_source";
pub(crate) const STORAGE_URL: &str = "storage_url";
pub(crate) const STORAGE_BUCKET: &str = "storage_bucket";
pub(crate) const KUBECONFIG_PATH: &str = "kubeconfig_path";
pub(crate) const KUBE_CONTEXT: &str = "kube_context";
const SELECTED_ITEMS: &str = "selected_items";

const URL_SCHEMES: [&str; 2] = ["https://", "http://"];

pub(crate) fn deck_url(config: &HashMap<String, String>) -> PluginResult<Url> {
    parse_http_url(
        value(config, DECK_URL)
            .unwrap_or(DEFAULT_DECK_URL)
            .trim_end_matches('/'),
        "Deck",
    )
}

pub(crate) fn is_insecure(config: &HashMap<String, String>) -> bool {
    value(config, INSECURE).is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

enum AuthScheme {
    Bearer,
    Basic,
    Cookie,
}

fn auth_scheme(config: &HashMap<String, String>) -> PluginResult<AuthScheme> {
    match value(config, AUTH_SCHEME) {
        None | Some("bearer") => Ok(AuthScheme::Bearer),
        Some("basic") => Ok(AuthScheme::Basic),
        Some("cookie") => Ok(AuthScheme::Cookie),
        Some(other) => Err(PluginError::InvalidConfig(format!(
            "Unknown token type '{other}'. Use bearer, basic or cookie"
        ))),
    }
}

pub(crate) fn auth_header(
    config: &HashMap<String, String>,
) -> PluginResult<Option<(HeaderName, HeaderValue)>> {
    let Some(token) = value(config, TOKEN) else {
        return Ok(None);
    };
    let (name, raw) = match auth_scheme(config)? {
        AuthScheme::Bearer => (AUTHORIZATION, format!("Bearer {token}")),
        AuthScheme::Basic => {
            if !token.contains(':') {
                return Err(PluginError::InvalidConfig(
                    "A basic token must look like user:password".to_string(),
                ));
            }
            let encoded = base64::engine::general_purpose::STANDARD.encode(token);
            (AUTHORIZATION, format!("Basic {encoded}"))
        }
        AuthScheme::Cookie => (COOKIE, token.to_string()),
    };
    let header = sensitive_header(&raw, "token")?;
    Ok(Some((name, header)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistorySource {
    Deck,
    Storage,
}

pub(crate) fn history_source(config: &HashMap<String, String>) -> PluginResult<HistorySource> {
    match value(config, HISTORY_SOURCE) {
        None | Some("deck") => Ok(HistorySource::Deck),
        Some("storage") => Ok(HistorySource::Storage),
        Some(other) => Err(PluginError::InvalidConfig(format!(
            "Unknown run history source '{other}'. Use deck or storage"
        ))),
    }
}

pub(crate) fn storage_url(config: &HashMap<String, String>) -> PluginResult<Option<Url>> {
    value(config, STORAGE_URL)
        .map(|raw| parse_http_url(raw.trim_end_matches('/'), "artifact storage"))
        .transpose()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bucket {
    pub provider: String,
    pub name: String,
}

pub(crate) fn storage_bucket(config: &HashMap<String, String>) -> PluginResult<Option<Bucket>> {
    let Some(raw) = value(config, STORAGE_BUCKET) else {
        return Ok(None);
    };
    let (provider, name) = raw.split_once("://").unwrap_or(("gs", raw));
    if !matches!(provider, "gs" | "s3") {
        return Err(PluginError::InvalidConfig(format!(
            "Unknown storage '{provider}://'. Use gs:// or s3://"
        )));
    }
    let name = name.trim_end_matches('/');
    if name.is_empty() || name.contains('/') {
        return Err(PluginError::InvalidConfig(
            "Enter only the bucket, for example gs://kubernetes-ci-logs".to_string(),
        ));
    }
    Ok(Some(Bucket {
        provider: provider.to_string(),
        name: name.to_string(),
    }))
}

#[derive(Debug, Clone)]
pub(crate) struct KubeSettings {
    pub paths: Vec<String>,
    pub context: Option<String>,
}

fn expand_path(path: &str) -> String {
    shellexpand::full(path)
        .map(|expanded| expanded.into_owned())
        .unwrap_or_else(|_| path.to_string())
}

pub(crate) fn kube_settings(config: &HashMap<String, String>) -> Option<KubeSettings> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let paths: Vec<String> = value(config, KUBECONFIG_PATH)?
        .split(separator)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(expand_path)
        .collect();
    if paths.is_empty() {
        return None;
    }
    Some(KubeSettings {
        paths,
        context: value(config, KUBE_CONTEXT).map(str::to_string),
    })
}

pub(crate) fn display_org(org: &str) -> &str {
    URL_SCHEMES
        .iter()
        .find_map(|scheme| org.strip_prefix(scheme))
        .unwrap_or(org)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Scope {
    pub org: String,
    pub repo: String,
}

impl Scope {
    pub(crate) fn repository(&self) -> String {
        format!("{}/{}", display_org(&self.org), self.repo)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    pub scope: Option<Scope>,
    pub job: String,
}

impl Selection {
    pub(crate) fn token(&self) -> String {
        match &self.scope {
            Some(scope) => format!("{}/{}/{}", scope.org, scope.repo, self.job),
            None => self.job.clone(),
        }
    }

    pub(crate) fn parse(token: &str) -> PluginResult<Self> {
        let token = token.trim();
        let invalid = || {
            PluginError::InvalidConfig(format!(
                "Invalid Prow selection '{token}'. Expected org/repo/job or job"
            ))
        };
        if token.is_empty() {
            return Err(invalid());
        }
        let Some((scope, job)) = token.rsplit_once('/') else {
            return Ok(Self {
                scope: None,
                job: token.to_string(),
            });
        };
        let (org, repo) = match URL_SCHEMES
            .iter()
            .find_map(|scheme| scope.strip_prefix(scheme).map(|rest| (scheme, rest)))
        {
            Some((scheme, rest)) => {
                let (host, repo) = rest.split_once('/').ok_or_else(invalid)?;
                if host.is_empty() {
                    return Err(invalid());
                }
                (format!("{scheme}{host}"), repo)
            }
            None => {
                let (org, repo) = scope.split_once('/').ok_or_else(invalid)?;
                (org.to_string(), repo)
            }
        };
        if org.is_empty() || repo.is_empty() || job.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            scope: Some(Scope {
                org,
                repo: repo.to_string(),
            }),
            job: job.to_string(),
        })
    }
}

pub(crate) fn selections(config: &HashMap<String, String>) -> PluginResult<Vec<Selection>> {
    let mut selections: Vec<Selection> = Vec::new();
    for token in comma_separated(
        config
            .get(SELECTED_ITEMS)
            .map(String::as_str)
            .unwrap_or_default(),
    ) {
        let selection = Selection::parse(token)?;
        if !selections.contains(&selection) {
            selections.push(selection);
        }
    }
    Ok(selections)
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<Selection> {
    Selection::parse(id).map_err(|_| {
        PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: '{{org}}/{{repo}}/{{job}}'"
        ))
    })
}

#[cfg(test)]
mod tests;
