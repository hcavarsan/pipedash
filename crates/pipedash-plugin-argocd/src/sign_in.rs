use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    SignInClient,
    SignInToken,
};

use crate::types::Settings;

const CLI_REDIRECT_URI: &str = "http://localhost:8085/auth/callback";
const DEX_CLIENT_ID: &str = "argo-cd-cli";
const DEX_PATH: &str = "/api/dex";
const DEFAULT_SCOPES: [&str; 4] = ["openid", "profile", "email", "groups"];
const DEX_FEDERATED_SCOPE: &str = "federated:id";
const GENERIC_LABEL: &str = "SSO";
const NO_SSO: &str =
    "This Argo CD server has no SSO configured. Use an authentication token instead.";

fn default_scopes() -> Vec<String> {
    DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect()
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    if value.is_empty() { fallback } else { value }.to_string()
}

pub(crate) fn client_for(settings: &Settings, server_url: &str) -> PluginResult<SignInClient> {
    let client = |label: String, issuer: String, client_id: String, scopes, params| SignInClient {
        label,
        issuer,
        client_id,
        scopes,
        redirect_uri: CLI_REDIRECT_URI.to_string(),
        authorize_params: params,
        token: SignInToken::IdToken,
    };

    if let Some(oidc) = settings
        .oidc_config
        .as_ref()
        .filter(|o| !o.issuer.is_empty())
    {
        let scopes = if oidc.scopes.is_empty() {
            default_scopes()
        } else {
            oidc.scopes.clone()
        };
        return Ok(client(
            non_empty_or(&oidc.name, GENERIC_LABEL),
            oidc.issuer.clone(),
            non_empty_or(&oidc.cli_client_id, &oidc.client_id),
            scopes,
            Vec::new(),
        ));
    }

    let Some(dex) = settings
        .dex_config
        .as_ref()
        .filter(|d| !d.connectors.is_empty())
    else {
        return Err(PluginError::NotSupported(NO_SSO.to_string()));
    };

    let base = non_empty_or(settings.url.trim_end_matches('/'), server_url);
    let label = match dex.connectors.as_slice() {
        [only] => non_empty_or(&only.name, GENERIC_LABEL),
        _ => GENERIC_LABEL.to_string(),
    };
    let mut scopes = default_scopes();
    scopes.push(DEX_FEDERATED_SCOPE.to_string());
    let params = if dex.auth_connector_id.is_empty() {
        Vec::new()
    } else {
        vec![("connector_id".to_string(), dex.auth_connector_id.clone())]
    };
    Ok(client(
        label,
        format!("{base}{DEX_PATH}"),
        DEX_CLIENT_ID.to_string(),
        scopes,
        params,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(json: &str) -> Settings {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn external_oidc_prefers_cli_client() {
        let client = client_for(
            &settings(
                r#"{"oidcConfig":{"name":"Okta","issuer":"https://idp.example.com","clientID":"web","cliClientID":"cli","scopes":["openid","groups"]}}"#,
            ),
            "https://argo.example.com",
        )
        .unwrap();
        assert_eq!(client.label, "Okta");
        assert_eq!(client.issuer, "https://idp.example.com");
        assert_eq!(client.client_id, "cli");
        assert_eq!(client.scopes, ["openid", "groups"]);
        assert_eq!(client.redirect_uri, CLI_REDIRECT_URI);
        assert_eq!(client.token, SignInToken::IdToken);
        assert!(client.authorize_params.is_empty());
    }

    #[test]
    fn external_oidc_falls_back_to_web_client_and_default_scopes() {
        let client = client_for(
            &settings(r#"{"oidcConfig":{"issuer":"https://idp.example.com","clientID":"web"}}"#),
            "https://argo.example.com",
        )
        .unwrap();
        assert_eq!(client.label, "SSO");
        assert_eq!(client.client_id, "web");
        assert_eq!(client.scopes, default_scopes());
    }

    #[test]
    fn dex_single_connector_uses_server_url_and_connector_name() {
        let client = client_for(
            &settings(
                r#"{"url":"","dexConfig":{"connectors":[{"name":"GitHub","type":"github"}]}}"#,
            ),
            "https://argo.example.com",
        )
        .unwrap();
        assert_eq!(client.label, "GitHub");
        assert_eq!(client.issuer, "https://argo.example.com/api/dex");
        assert_eq!(client.client_id, "argo-cd-cli");
        assert_eq!(client.scopes.last().unwrap(), "federated:id");
        assert!(client.authorize_params.is_empty());
    }

    #[test]
    fn dex_many_connectors_use_generic_label_and_connector_param() {
        let client = client_for(
            &settings(
                r#"{"url":"https://argo.public.example.com/","dexConfig":{"connectors":[{"name":"GitHub"},{"name":"LDAP"}],"dexAuthConnectorID":"ldap"}}"#,
            ),
            "https://argo.internal",
        )
        .unwrap();
        assert_eq!(client.label, "SSO");
        assert_eq!(client.issuer, "https://argo.public.example.com/api/dex");
        assert_eq!(
            client.authorize_params,
            [("connector_id".to_string(), "ldap".to_string())]
        );
    }

    #[test]
    fn no_sso_is_not_supported() {
        let error = client_for(
            &settings(r#"{"url":"https://a.io","dexConfig":{}}"#),
            "https://a.io",
        )
        .unwrap_err();
        assert!(matches!(error, PluginError::NotSupported(message) if message == NO_SSO));
    }
}
