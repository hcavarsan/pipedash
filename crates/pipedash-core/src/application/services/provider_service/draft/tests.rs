use pipedash_plugin_api::SignInToken;

use super::*;
use crate::application::test_support::plugins::{
    self,
    STUB_DRAFT,
};
use crate::application::test_support::Harness;
use crate::domain::SignInCredential;

const VALID: [(&str, &str); 2] = [("deck_url", "https://deck.example"), ("token", "abc")];

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn draft(mode: AuthMode, values: &[(&str, &str)]) -> ProviderDraft {
    ProviderDraft {
        provider_type: STUB_DRAFT.to_string(),
        name: "Stub".to_string(),
        auth_mode: mode,
        values: map(values),
        selected_pipeline_ids: None,
        provider_id: None,
    }
}

fn editing(id: i64, values: &[(&str, &str)]) -> ProviderDraft {
    ProviderDraft {
        provider_id: Some(id),
        ..draft(AuthMode::Token, values)
    }
}

fn stored(values: &[(&str, &str)]) -> ProviderConfig {
    ProviderConfig {
        id: None,
        name: "Stub".to_string(),
        provider_type: STUB_DRAFT.to_string(),
        token: "stored".to_string(),
        config: map(values),
        refresh_interval: 45,
        version: Some(3),
    }
}

fn config_of(draft: &ProviderDraft, stored: Option<&ProviderConfig>) -> ProviderConfig {
    let registry = plugins::registry();
    let metadata = registry.get(STUB_DRAFT).unwrap().metadata();
    rules::provider_config(metadata, draft, stored)
}

async fn validate(h: &Harness, draft: &ProviderDraft) -> ProviderDraftResult {
    h.provider_service.validate_draft(draft).await.unwrap()
}

fn keys(result: &ProviderDraftResult) -> Vec<&str> {
    result
        .active_field_keys
        .iter()
        .map(String::as_str)
        .collect()
}

async fn signed_in_provider(h: &Harness) -> i64 {
    let credential = SignInCredential {
        token: "bearer".into(),
        refresh_token: None,
        expires_at: None,
        token_endpoint: String::new(),
        client_id: "cli".into(),
        kind: SignInToken::AccessToken,
        account: Some("dev@example.com".into()),
    };
    let mut config = stored(&[]);
    config.token = credential.encode().unwrap();
    h.repository.add_provider(&config).await.unwrap()
}

#[tokio::test]
async fn unknown_provider_type_is_rejected() {
    let h = Harness::new().await;
    let mut unknown = draft(AuthMode::Token, &[]);
    unknown.provider_type = "missing".to_string();

    assert!(matches!(
        h.provider_service.validate_draft(&unknown).await,
        Err(DomainError::InvalidProviderType(_))
    ));
    assert!(matches!(
        h.provider_service.save_draft(unknown).await,
        Err(DomainError::InvalidProviderType(_))
    ));
}

#[tokio::test]
async fn sign_in_mode_drops_the_token_field_and_needs_a_completed_sign_in() {
    let h = Harness::new().await;
    let base = [("deck_url", "https://deck.example"), ("sso_issuer", "i")];
    let with_sign_in = |reference: &str| {
        let mut values = base.to_vec();
        values.push(("sign_in", reference));
        draft(AuthMode::SignIn, &values)
    };

    let missing = validate(&h, &draft(AuthMode::SignIn, &base)).await;
    assert!(!keys(&missing).contains(&"token"));
    assert_eq!(missing.errors["sign_in"], "Sign in to continue");
    assert!(!missing.credentials_complete);

    let token = validate(&h, &draft(AuthMode::Token, &[])).await;
    assert!(keys(&token).contains(&"token"));
    assert!(!token.errors.contains_key("sign_in"));

    let pending = validate(&h, &with_sign_in("unknown-flow")).await;
    assert_eq!(pending.errors["sign_in"], "Finish signing in to continue");
    assert!(!pending.credentials_complete);

    let reference = format!("provider:{}", signed_in_provider(&h).await);
    let done = validate(&h, &with_sign_in(&reference)).await;
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    assert!(done.credentials_complete);
}

#[tokio::test]
async fn whitespace_fails_when_required_and_empty_passes_when_optional() {
    let h = Harness::new().await;
    let result = validate(
        &h,
        &draft(
            AuthMode::Token,
            &[
                ("deck_url", "https://deck.example"),
                ("token", "   "),
                ("note", ""),
            ],
        ),
    )
    .await;

    assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
    assert_eq!(result.errors["token"], "Token is required");
    assert_eq!(result.missing_field_labels, ["Token"]);
}

#[tokio::test]
async fn regex_mismatch_uses_the_custom_message_then_the_fallback() {
    let h = Harness::new().await;
    let check = |deck_url: &str, pattern: &str| {
        draft(
            AuthMode::Token,
            &[
                ("deck_url", deck_url),
                ("token", "abc"),
                ("pattern", pattern),
            ],
        )
    };

    let valid = validate(&h, &check("https://a.example", "https://b.example")).await;
    assert!(valid.errors.is_empty(), "{:?}", valid.errors);

    let invalid = validate(&h, &check("http://a.example", "http://b.example")).await;
    assert_eq!(invalid.errors["deck_url"], "Use https");
    assert_eq!(invalid.errors["pattern"], "Invalid format for Pattern");
    assert!(invalid.credentials_complete);
}

#[tokio::test]
async fn provider_name_must_have_non_whitespace_content() {
    let h = Harness::new().await;
    let mut blank = draft(AuthMode::Token, &VALID);
    blank.name = "  ".to_string();

    let result = validate(&h, &blank).await;
    assert_eq!(result.errors["providerName"], "Provider name is required");
    assert!(result.credentials_complete);
    assert!(validate(&h, &draft(AuthMode::Token, &VALID))
        .await
        .errors
        .is_empty());
}

#[tokio::test]
async fn credentials_draft_collects_name_field_and_sign_in_errors_together() {
    let h = Harness::new().await;
    let mut empty = draft(AuthMode::Token, &[]);
    empty.name = String::new();

    let result = validate(&h, &empty).await;
    assert_eq!(
        result.errors,
        BTreeMap::from([
            ("deck_url".to_string(), "Deck URL is required".to_string()),
            (
                "providerName".to_string(),
                "Provider name is required".to_string()
            ),
            ("token".to_string(), "Token is required".to_string()),
        ])
    );
    assert_eq!(result.missing_field_labels, ["Deck URL", "Token"]);
    assert!(!result.credentials_complete);

    let sign_in = validate(
        &h,
        &draft(
            AuthMode::SignIn,
            &[("deck_url", "https://deck.example"), ("sso_issuer", "i")],
        ),
    )
    .await;
    assert_eq!(
        sign_in.errors,
        BTreeMap::from([("sign_in".to_string(), "Sign in to continue".to_string())])
    );
}

#[tokio::test]
async fn auth_scoped_fields_count_only_in_their_own_mode() {
    let h = Harness::new().await;
    let token = validate(&h, &draft(AuthMode::Token, &VALID)).await;
    let sign_in = validate(&h, &draft(AuthMode::SignIn, &[])).await;
    let none = validate(
        &h,
        &draft(AuthMode::None, &[("deck_url", "https://d.example")]),
    )
    .await;

    assert!(keys(&token).contains(&"auth_scheme") && keys(&token).contains(&"token"));
    assert!(!keys(&token).contains(&"sso_issuer"));
    assert!(token.errors.is_empty(), "{:?}", token.errors);
    assert!(keys(&sign_in).contains(&"sso_issuer") && keys(&sign_in).contains(&"auth_scheme"));
    assert!(!keys(&sign_in).contains(&"token"));
    assert_eq!(sign_in.errors["sso_issuer"], "SSO Issuer is required");
    assert!(!keys(&none).contains(&"token"));
    assert!(!keys(&none).contains(&"sso_issuer") && !keys(&none).contains(&"auth_scheme"));
    assert!(keys(&none).contains(&"deck_url"));
    assert!(none.errors.is_empty() && none.credentials_complete);
}

#[tokio::test]
async fn a_malformed_or_oversized_validation_regex_is_reported_on_its_field() {
    let h = Harness::new().await;
    let result = validate(
        &h,
        &draft(
            AuthMode::Token,
            &[("token", "abc"), ("broken", "x"), ("huge", "a")],
        ),
    )
    .await;
    let message = |label: &str| {
        format!("The validation rule for {label} is invalid. Contact the plugin author.")
    };

    assert_eq!(result.errors["broken"], message("Broken"));
    assert_eq!(result.errors["huge"], message("Huge"));
    assert_eq!(result.errors["deck_url"], "Deck URL is required");

    let blank = validate(&h, &draft(AuthMode::Token, &VALID)).await;
    assert!(blank.errors.is_empty(), "{:?}", blank.errors);
}

#[tokio::test]
async fn an_empty_selection_names_the_plugin_item() {
    let h = Harness::new().await;
    for ids in [vec![], vec![" ".to_string()]] {
        let mut selecting = draft(AuthMode::Token, &VALID);
        selecting.selected_pipeline_ids = Some(ids);
        let result = validate(&h, &selecting).await;

        assert_eq!(
            result.errors["selected_items"],
            "Please select at least one pipeline"
        );
        assert!(matches!(
            h.provider_service.save_draft(selecting).await,
            Err(DomainError::InvalidConfig(message)) if message == "Please select at least one pipeline"
        ));
    }
    assert!(h.repository.list_providers().await.unwrap().is_empty());
}

#[tokio::test]
async fn invalid_drafts_are_never_persisted() {
    let h = Harness::new().await;
    let mut blank = draft(AuthMode::Token, &VALID);
    blank.name = " ".to_string();

    assert!(matches!(
        h.provider_service.save_draft(blank).await,
        Err(DomainError::InvalidConfig(message)) if message == "Provider name is required"
    ));
    assert!(h.repository.list_providers().await.unwrap().is_empty());
}

#[tokio::test]
async fn saving_a_new_draft_extracts_the_token_and_encodes_the_selection() {
    let h = Harness::new().await;
    let mut new = draft(
        AuthMode::Token,
        &[
            ("deck_url", "https://deck.example"),
            ("token", "abc"),
            ("sso_issuer", "i"),
            ("sign_in", "flow"),
            ("auth_mode", "none"),
        ],
    );
    new.selected_pipeline_ids = Some(vec!["a".into(), " b".into(), "a".into(), String::new()]);

    let id = h.provider_service.save_draft(new).await.unwrap();
    let saved = h.repository.get_provider(id).await.unwrap();

    assert_eq!(saved.token, "abc");
    assert_eq!(saved.refresh_interval, 30);
    assert_eq!(
        saved.config,
        map(&[
            ("deck_url", "https://deck.example"),
            ("selected_items", "a,b")
        ])
    );
}

#[tokio::test]
async fn updating_a_draft_keeps_the_interval_and_returns_the_id() {
    let h = Harness::new().await;
    let id = h
        .repository
        .add_provider(&stored(&[
            ("namespaces", "team-a"),
            ("selected_items", "x"),
        ]))
        .await
        .unwrap();
    let mut edit = editing(
        id,
        &[
            ("deck_url", "https://deck.example"),
            ("token", "new"),
            ("namespaces", "team-a"),
            ("selected_items", "x"),
        ],
    );
    edit.name = "Renamed".to_string();

    let result = validate(&h, &edit).await;
    assert!(!result.selection_needs_refresh);
    assert_eq!(h.provider_service.save_draft(edit).await.unwrap(), id);

    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(saved.name, "Renamed");
    assert_eq!(saved.token, "new");
    assert_eq!(saved.refresh_interval, 45);
    assert_eq!(saved.config["selected_items"], "x");
}

#[tokio::test]
async fn a_scope_change_drops_the_saved_selection() {
    let h = Harness::new().await;
    let id = h
        .repository
        .add_provider(&stored(&[
            ("namespaces", "team-a"),
            ("selected_items", "x"),
        ]))
        .await
        .unwrap();
    let values = [
        ("deck_url", "https://deck.example"),
        ("token", "abc"),
        ("namespaces", "team-b"),
        ("selected_items", "x"),
    ];

    assert!(
        validate(&h, &editing(id, &values))
            .await
            .selection_needs_refresh
    );
    h.provider_service
        .save_draft(editing(id, &values))
        .await
        .unwrap();
    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(saved.config["namespaces"], "team-b");
    assert!(!saved.config.contains_key("selected_items"));

    let mut reselect = editing(id, &values);
    reselect.selected_pipeline_ids = Some(vec!["y".to_string()]);
    h.provider_service.save_draft(reselect).await.unwrap();
    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(saved.config["selected_items"], "y");
}

#[test]
fn token_mode_extracts_the_token_and_drops_other_credentials() {
    let config = config_of(
        &draft(
            AuthMode::Token,
            &[
                ("token", "abc"),
                ("sign_in", "provider:1"),
                ("auth_mode", "none"),
                ("deck_url", "https://d"),
                ("sso_issuer", "i"),
                ("auth_scheme", "basic"),
            ],
        ),
        None,
    );

    assert_eq!(config.token, "abc");
    assert_eq!(
        config.config,
        map(&[("deck_url", "https://d"), ("auth_scheme", "basic")])
    );
    assert_eq!(
        (config.refresh_interval, config.version, config.id),
        (30, None, None)
    );
}

#[test]
fn sign_in_mode_drops_the_token_and_keeps_the_sign_in_reference() {
    let config = config_of(
        &draft(
            AuthMode::SignIn,
            &[
                ("token", "abc"),
                ("sign_in", "provider:3"),
                ("deck_url", "https://d"),
                ("sso_issuer", "i"),
            ],
        ),
        None,
    );

    assert_eq!(config.token, "");
    assert_eq!(
        config.config,
        map(&[
            ("sign_in", "provider:3"),
            ("deck_url", "https://d"),
            ("sso_issuer", "i")
        ])
    );
}

#[test]
fn none_mode_keeps_unscoped_fields_and_tracks_auth_mode() {
    let config = config_of(
        &draft(
            AuthMode::None,
            &[
                ("token", "abc"),
                ("sign_in", "provider:1"),
                ("deck_url", "https://d"),
                ("sso_issuer", "i"),
                ("auth_scheme", "basic"),
            ],
        ),
        None,
    );

    assert_eq!(config.token, "");
    assert_eq!(
        config.config,
        map(&[("deck_url", "https://d"), ("auth_mode", "none")])
    );
}

#[test]
fn selected_items_encoding_follows_the_draft_selection() {
    let mut selected = draft(AuthMode::Token, &[("selected_items", "stale")]);
    selected.selected_pipeline_ids =
        Some(vec![" a ".into(), "b".into(), "a".into(), String::new()]);
    assert_eq!(config_of(&selected, None).config["selected_items"], "a,b");

    let kept = draft(AuthMode::Token, &[("selected_items", "x, y")]);
    assert_eq!(config_of(&kept, None).config["selected_items"], "x, y");

    assert!(!config_of(&draft(AuthMode::Token, &[]), None)
        .config
        .contains_key("selected_items"));
}

#[test]
fn changing_the_kubeconfig_clears_its_previous_context() {
    let saved = stored(&[("kubeconfig", "/old/config"), ("context", "production")]);
    let moved = editing(
        1,
        &[("kubeconfig", "/new/config"), ("context", "production")],
    );
    let config = config_of(&moved, Some(&saved));

    assert_eq!(
        config.config,
        map(&[("kubeconfig", "/new/config"), ("context", "")])
    );
    assert_eq!(
        (config.id, config.refresh_interval, config.version),
        (Some(1), 45, Some(3))
    );

    let chosen = editing(1, &[("kubeconfig", "/new/config"), ("context", "staging")]);
    assert_eq!(
        config_of(&chosen, Some(&saved)).config["context"],
        "staging"
    );
}

#[test]
fn changing_a_scope_key_clears_saved_resource_selections() {
    for key in ["kubeconfig", "context", "namespaces"] {
        let saved = stored(&[(key, "old-scope"), ("selected_items", "id-1")]);
        let changed = editing(1, &[(key, "new-scope"), ("selected_items", "id-1")]);

        assert!(
            rules::selection_needs_refresh(&changed, Some(&saved)),
            "{key}"
        );
        assert!(!config_of(&changed, Some(&saved))
            .config
            .contains_key("selected_items"));
    }
}

#[test]
fn editing_credentials_without_changing_scope_preserves_the_selection() {
    let saved = stored(&[("namespaces", "team-a"), ("selected_items", "id-1")]);
    for values in [
        vec![
            ("token", "updated"),
            ("namespaces", "team-a"),
            ("selected_items", "id-1"),
        ],
        vec![("namespaces", "team-a"), ("selected_items", "id-1")],
    ] {
        let unchanged = editing(1, &values);

        assert!(!rules::selection_needs_refresh(&unchanged, Some(&saved)));
        assert_eq!(
            config_of(&unchanged, Some(&saved)).config["selected_items"],
            "id-1"
        );
    }
}

#[test]
fn a_missing_scope_value_equals_an_empty_one() {
    let saved = stored(&[("selected_items", "id-1")]);
    let blank = editing(1, &[("namespaces", ""), ("selected_items", "id-1")]);

    assert!(!rules::selection_needs_refresh(&blank, Some(&saved)));
}

#[test]
fn a_draft_without_a_selection_needs_a_refresh_only_when_one_was_saved() {
    let saved = stored(&[("selected_items", "id-1")]);
    let mut lost = editing(1, &[]);

    assert!(rules::selection_needs_refresh(&lost, Some(&saved)));
    lost.selected_pipeline_ids = Some(vec![]);
    assert!(rules::selection_needs_refresh(&lost, Some(&saved)));
    lost.selected_pipeline_ids = Some(vec!["id-2".to_string()]);
    assert!(!rules::selection_needs_refresh(&lost, Some(&saved)));

    let nothing_saved = stored(&[("namespaces", "a")]);
    assert!(!rules::selection_needs_refresh(
        &editing(1, &[("namespaces", "b")]),
        Some(&nothing_saved)
    ));
    assert!(!rules::selection_needs_refresh(
        &draft(AuthMode::Token, &[]),
        None
    ));
}

#[test]
fn debug_output_never_prints_values() {
    let secret = draft(
        AuthMode::Token,
        &[("token", "secret-token"), ("sign_in", "flow-secret")],
    );
    let printed = format!("{secret:?}");

    assert!(!printed.contains("secret-token") && !printed.contains("flow-secret"));
    assert!(printed.contains("token") && printed.contains("sign_in"));
}
