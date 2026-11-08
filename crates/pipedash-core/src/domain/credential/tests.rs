use super::*;

fn credential(refresh_token: Option<&str>) -> SignInCredential {
    SignInCredential {
        token: "id-token-secret".into(),
        refresh_token: refresh_token.map(Into::into),
        expires_at: Some(DateTime::from_timestamp(1_800_000_000, 0).unwrap()),
        token_endpoint: "https://idp.example.com/token".into(),
        client_id: "cli".into(),
        kind: SignInToken::IdToken,
        account: Some("dev@example.com".into()),
    }
}

#[test]
fn encoded_credential_round_trips_and_exposes_bearer() {
    let stored = credential(Some("refresh-secret")).encode().unwrap();
    assert_eq!(
        SignInCredential::decode(&stored),
        Some(credential(Some("refresh-secret")))
    );
    assert_eq!(SignInCredential::bearer(&stored), "id-token-secret");
}

#[test]
fn plain_token_is_not_a_credential() {
    assert_eq!(SignInCredential::decode("stubtok_plain"), None);
    assert_eq!(SignInCredential::bearer("stubtok_plain"), "stubtok_plain");
}

#[test]
fn debug_hides_tokens() {
    let printed = format!("{:?}", credential(Some("refresh-secret")));
    assert!(!printed.contains("id-token-secret"));
    assert!(!printed.contains("refresh-secret"));
    assert!(printed.contains("dev@example.com"));
}

#[test]
fn renews_a_minute_before_expiry_only_with_refresh_token() {
    assert_eq!(credential(None).renew_at(), None);
    let with_refresh = credential(Some("r"));
    assert_eq!(
        with_refresh.renew_at(),
        Some(with_refresh.expires_at.unwrap() - Duration::seconds(60))
    );
}

#[test]
fn reference_parses_provider_and_flow() {
    assert_eq!(SignInRef::parse("provider:7"), SignInRef::Provider(7));
    assert_eq!(SignInRef::Provider(7).to_string(), "provider:7");
    assert_eq!(
        SignInRef::parse("Xy-z_09"),
        SignInRef::Flow("Xy-z_09".into())
    );
}
