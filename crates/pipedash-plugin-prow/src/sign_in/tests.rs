use super::*;

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn sign_in_client_uses_custom_settings() {
    let client = client_for(&config(&[
        ("sso_issuer", " https://dex.example.com/dex/ "),
        ("sso_client_id", "pipedash"),
        ("sso_params", "audience=deck& connector_id=github"),
    ]))
    .unwrap();
    assert_eq!(client.issuer, "https://dex.example.com/dex");
    assert_eq!(client.label, "dex.example.com");
    assert_eq!(client.scopes, ["openid", "email", "profile"]);
    assert_eq!(client.redirect_uri, "http://localhost:8085/auth/callback");
    assert_eq!(client.token, SignInToken::IdToken);
    assert_eq!(
        client.authorize_params,
        [
            ("audience".to_string(), "deck".to_string()),
            ("connector_id".to_string(), "github".to_string())
        ]
    );

    let client = client_for(&config(&[
        ("sso_issuer", "https://accounts.google.com"),
        ("sso_client_id", "id"),
        ("sso_scopes", "email,groups email"),
    ]))
    .unwrap();
    assert_eq!(client.scopes, ["openid", "email", "groups"]);

    let client = client_for(&config(&[
        ("sso_issuer", "https://idp.example.com"),
        ("sso_client_id", "id"),
        ("sso_scopes", "deck.read"),
        ("sso_token", "access_token"),
    ]))
    .unwrap();
    assert_eq!(client.scopes, ["deck.read"]);
    assert_eq!(client.token, SignInToken::AccessToken);

    assert!(client_for(&config(&[("sso_client_id", "id")])).is_err());
    assert!(client_for(&config(&[
        ("sso_issuer", "https://idp.example.com"),
        ("sso_client_id", "id"),
        ("sso_params", "novalue"),
    ]))
    .is_err());
}
