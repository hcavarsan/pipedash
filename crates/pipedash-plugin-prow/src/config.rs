use std::collections::HashMap;

use base64::Engine;
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

pub(crate) const PROVIDER_TYPE: &str = "prow";
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

const PIPELINE_ID_SEPARATOR: &str = "__";
const URL_SCHEMES: [&str; 2] = ["https://", "http://"];

pub(crate) fn value<'a>(config: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    config
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn http_url(raw: &str, label: &str) -> PluginResult<Url> {
    let url = Url::parse(raw.trim_end_matches('/'))
        .map_err(|e| PluginError::InvalidConfig(format!("Invalid {label}: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(PluginError::InvalidConfig(format!(
            "The {label} must start with http:// or https://"
        )));
    }
    Ok(url)
}

pub(crate) fn deck_url(config: &HashMap<String, String>) -> PluginResult<Url> {
    http_url(
        value(config, DECK_URL).unwrap_or(DEFAULT_DECK_URL),
        "Deck URL",
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
    let mut header = HeaderValue::from_str(&raw).map_err(|_| {
        PluginError::InvalidConfig(
            "The token contains characters that can't be sent in an HTTP header".to_string(),
        )
    })?;
    header.set_sensitive(true);
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
        .map(|raw| http_url(raw, "artifact storage URL"))
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
    for token in config
        .get(SELECTED_ITEMS)
        .map(String::as_str)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let selection = Selection::parse(token)?;
        if !selections.contains(&selection) {
            selections.push(selection);
        }
    }
    Ok(selections)
}

pub(crate) fn build_pipeline_id(provider_id: i64, selection: &Selection) -> String {
    format!(
        "{PROVIDER_TYPE}{PIPELINE_ID_SEPARATOR}{provider_id}{PIPELINE_ID_SEPARATOR}{}",
        selection.token()
    )
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<Selection> {
    let invalid = || {
        PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: 'prow__{{provider_id}}__{{org}}/{{repo}}/{{job}}'"
        ))
    };
    let mut parts = id.splitn(3, PIPELINE_ID_SEPARATOR);
    let (Some(PROVIDER_TYPE), Some(provider_id), Some(token)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    provider_id.parse::<i64>().map_err(|_| invalid())?;
    Selection::parse(token).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn selection_tokens_round_trip_github_gerrit_and_refless_jobs() {
        let github = Selection::parse("kubernetes/test-infra/ci-x").unwrap();
        assert_eq!(
            github.scope,
            Some(Scope {
                org: "kubernetes".into(),
                repo: "test-infra".into()
            })
        );
        assert_eq!(github.job, "ci-x");

        let gerrit =
            Selection::parse("https://gerrit.example.com/platform/build/pull-build").unwrap();
        let scope = gerrit.scope.clone().unwrap();
        assert_eq!(scope.org, "https://gerrit.example.com");
        assert_eq!(scope.repo, "platform/build");
        assert_eq!(scope.repository(), "gerrit.example.com/platform/build");
        assert_eq!(gerrit.job, "pull-build");

        let refless = Selection::parse("ci-noref").unwrap();
        assert_eq!(refless.scope, None);

        for selection in [github, gerrit, refless] {
            let id = build_pipeline_id(7, &selection);
            assert_eq!(parse_pipeline_id(&id).unwrap(), selection);
            assert_eq!(Selection::parse(&selection.token()).unwrap(), selection);
        }

        assert!(Selection::parse("a/b").is_err());
        assert!(Selection::parse("").is_err());
        assert!(parse_pipeline_id("komodo__1__a/b/c").is_err());
        assert!(parse_pipeline_id("prow__x__a/b/c").is_err());
    }

    #[test]
    fn auth_header_supports_bearer_basic_and_cookie() {
        let (name, value) = auth_header(&config(&[("token", "abc")])).unwrap().unwrap();
        assert_eq!(name, AUTHORIZATION);
        assert_eq!(value, "Bearer abc");
        assert!(value.is_sensitive());

        let (_, value) = auth_header(&config(&[("token", "user:pass"), ("auth_scheme", "basic")]))
            .unwrap()
            .unwrap();
        assert_eq!(value, "Basic dXNlcjpwYXNz");

        assert!(auth_header(&config(&[("token", "nopass"), ("auth_scheme", "basic")])).is_err());

        let (name, value) = auth_header(&config(&[
            ("token", "_oauth2_proxy=xyz"),
            ("auth_scheme", "cookie"),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(name, COOKIE);
        assert_eq!(value, "_oauth2_proxy=xyz");

        assert!(auth_header(&config(&[("token", "  ")])).unwrap().is_none());
        assert!(auth_header(&config(&[("token", "a"), ("auth_scheme", "digest")])).is_err());
    }
}
