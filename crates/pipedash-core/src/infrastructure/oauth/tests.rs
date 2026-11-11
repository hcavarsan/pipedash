use serde_json::json;

use super::test_idp::{
    jwt,
    Idp,
};
use super::*;

fn client(token: SignInToken) -> SignInClient {
    SignInClient {
        label: "Dex".into(),
        issuer: "https://idp.example.com".into(),
        client_id: "stub-cli".into(),
        scopes: vec!["openid".into(), "email".into()],
        redirect_uri: "http://localhost:8085/auth/callback".into(),
        authorize_params: vec![("connector_id".into(), "stub".into())],
        token,
    }
}

fn endpoints(scopes_supported: Option<Vec<String>>) -> Endpoints {
    Endpoints {
        authorization: Url::parse("https://idp.example.com/auth").unwrap(),
        token: Url::parse("https://idp.example.com/token").unwrap(),
        scopes_supported,
    }
}

fn query(url: &Url) -> Vec<(String, String)> {
    url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

#[test]
fn authorize_uses_pkce_s256_and_requests_offline_access_once() {
    let authorization = authorize(&client(SignInToken::IdToken), &endpoints(None));
    let pairs = query(&authorization.url);
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap()
    };

    assert_eq!(get("response_type"), "code");
    assert_eq!(get("scope"), "openid email offline_access");
    assert_eq!(get("state"), authorization.state);
    assert_eq!(get("code_challenge_method"), "S256");
    assert_eq!(
        get("code_challenge"),
        URL_SAFE_NO_PAD.encode(Sha256::digest(authorization.verifier.as_bytes()))
    );
    assert_eq!(
        pairs.last().unwrap(),
        &("connector_id".into(), "stub".into())
    );

    let mut with_offline = client(SignInToken::IdToken);
    with_offline.scopes.push(OFFLINE_ACCESS.into());
    let again = authorize(&with_offline, &endpoints(None));
    assert_eq!(
        query(&again.url)
            .iter()
            .find(|(k, _)| k == "scope")
            .unwrap()
            .1,
        "openid email offline_access"
    );
    let unsupported = authorize(
        &client(SignInToken::IdToken),
        &endpoints(Some(vec!["openid".into()])),
    );
    assert!(!unsupported.url.as_str().contains(OFFLINE_ACCESS));
}

#[tokio::test]
async fn exchange_posts_verifier_and_reads_id_token() {
    let idp = Idp::start().await;
    let id_token = jwt(json!({"exp": 1_900_000_000, "email": "dev@example.com", "sub": "1"}));
    idp.reply(
        200,
        json!({"access_token": "opaque", "id_token": id_token, "refresh_token": "r1", "expires_in": 60}),
    );
    let endpoint = Url::parse(&idp.token_endpoint()).unwrap();

    let credential = exchange(
        &reqwest::Client::new(),
        &endpoint,
        &client(SignInToken::IdToken),
        "the-code",
        "the-verifier",
    )
    .await
    .unwrap();

    assert_eq!(credential.token, id_token);
    assert_eq!(credential.refresh_token.as_deref(), Some("r1"));
    assert_eq!(credential.account.as_deref(), Some("dev@example.com"));
    assert_eq!(
        credential.expires_at,
        DateTime::from_timestamp(1_900_000_000, 0)
    );
    let form = idp.forms.lock().unwrap()[0].clone();
    assert!(form.contains("grant_type=authorization_code"));
    assert!(form.contains("code=the-code"));
    assert!(form.contains("code_verifier=the-verifier"));
    assert!(form.contains("redirect_uri=http%3A%2F%2Flocalhost%3A8085%2Fauth%2Fcallback"));
}

#[tokio::test]
async fn exchange_reports_the_identity_provider_error() {
    let idp = Idp::start().await;
    idp.reply(
        400,
        json!({"error": "invalid_request", "error_description": "Bad code"}),
    );
    let endpoint = Url::parse(&idp.token_endpoint()).unwrap();

    let error = exchange(
        &reqwest::Client::new(),
        &endpoint,
        &client(SignInToken::IdToken),
        "c",
        "v",
    )
    .await
    .unwrap_err();

    assert!(matches!(error, DomainError::ProviderAuthenticationFailed(m) if m == "Bad code"));
}

fn stored(idp: &Idp) -> SignInCredential {
    SignInCredential {
        token: "old".into(),
        refresh_token: Some("r1".into()),
        expires_at: None,
        token_endpoint: idp.token_endpoint(),
        client_id: "cli".into(),
        kind: SignInToken::AccessToken,
        account: Some("dev@example.com".into()),
    }
}

#[tokio::test]
async fn refresh_keeps_previous_refresh_token_and_account() {
    let idp = Idp::start().await;
    idp.reply(200, json!({"access_token": "new", "expires_in": 3600}));

    let renewed = refresh(&reqwest::Client::new(), &stored(&idp), &[])
        .await
        .unwrap();

    assert_eq!(renewed.token, "new");
    assert_eq!(renewed.refresh_token.as_deref(), Some("r1"));
    assert_eq!(renewed.account.as_deref(), Some("dev@example.com"));
    assert!(renewed.expires_at.unwrap() > Utc::now() + Duration::minutes(59));
    let form = idp.forms.lock().unwrap()[0].clone();
    assert!(form.contains("grant_type=refresh_token"));
    assert!(form.contains("refresh_token=r1"));
}

#[tokio::test]
async fn refresh_with_revoked_grant_asks_to_sign_in_again() {
    let idp = Idp::start().await;
    idp.reply(400, json!({"error": "invalid_grant"}));

    let error = refresh(&reqwest::Client::new(), &stored(&idp), &[])
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::ProviderAuthenticationFailed(m) if m == SIGN_IN_EXPIRED));
}

#[tokio::test]
async fn refresh_server_error_stays_transient() {
    let idp = Idp::start().await;
    idp.reply(503, json!({"error": "temporarily_unavailable"}));

    let error = refresh(&reqwest::Client::new(), &stored(&idp), &[])
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::ServiceUnavailable { .. }));
}

#[test]
fn sign_in_endpoints_must_be_web_addresses() {
    for allowed in [
        "https://idp.example.com/auth",
        "http://localhost:5556/dex/auth",
    ] {
        assert!(web_endpoint(Url::parse(allowed).unwrap()).is_ok());
    }
    for rejected in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "pipedash://open",
    ] {
        assert!(matches!(
            web_endpoint(Url::parse(rejected).unwrap()),
            Err(DomainError::InvalidConfig(_))
        ));
    }
}

#[test]
fn callback_reads_query_pairs() {
    let callback = Callback::from_url(
        &Url::parse("http://localhost:8085/auth/callback?code=c&state=s&extra=1").unwrap(),
    );
    assert_eq!(callback.code.as_deref(), Some("c"));
    assert_eq!(callback.state.as_deref(), Some("s"));
    assert_eq!(callback.error, None);
}
