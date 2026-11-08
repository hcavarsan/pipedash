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
        &settings(r#"{"url":"","dexConfig":{"connectors":[{"name":"GitHub","type":"github"}]}}"#),
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
