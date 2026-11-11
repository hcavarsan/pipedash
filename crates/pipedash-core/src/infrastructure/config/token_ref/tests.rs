use super::*;
use crate::application::test_support::plugins::{
    registry,
    STUB_TOKEN_PREFIXES,
};

fn parse(value: &str) -> Result<TokenReference, TokenRefError> {
    TokenReference::parse(value, &registry())
}

#[test]
fn test_parse_env_var_dollar_brace() {
    let result = parse("${STUB_TOKEN}").unwrap();
    assert_eq!(result, TokenReference::EnvVar("STUB_TOKEN".to_string()));
}

#[test]
fn test_parse_env_var_with_default() {
    let result = parse("${STUB_TOKEN:-}").unwrap();
    assert_eq!(result, TokenReference::EnvVar("STUB_TOKEN".to_string()));

    let result = parse("${STUB_TOKEN:-default_value}").unwrap();
    assert_eq!(result, TokenReference::EnvVar("STUB_TOKEN".to_string()));
}

#[test]
fn test_parse_env_prefix() {
    let result = parse("env:MY_TOKEN").unwrap();
    assert_eq!(result, TokenReference::EnvVar("MY_TOKEN".to_string()));
}

#[test]
fn test_parse_keyring() {
    let result = parse("keyring:my-provider").unwrap();
    assert_eq!(result, TokenReference::Keyring("my-provider".to_string()));
}

#[test]
fn test_parse_storage() {
    let result = parse("storage:123").unwrap();
    assert_eq!(result, TokenReference::SecureStorage(123));
}

#[test]
fn test_parse_empty() {
    let result = parse("").unwrap();
    assert_eq!(result, TokenReference::None);
}

#[test]
fn test_parse_uppercase_as_env() {
    let result = parse("STUB_TOKEN").unwrap();
    assert_eq!(result, TokenReference::EnvVar("STUB_TOKEN".to_string()));
}

#[test]
fn test_reject_registry_declared_prefixes_as_plain_text() {
    for prefix in STUB_TOKEN_PREFIXES {
        let result = parse(&format!("{prefix}xxxxxxxx"));
        assert!(
            matches!(result, Err(TokenRefError::PlainTextToken(_))),
            "{prefix}"
        );
    }
}

#[test]
fn test_registry_prefix_in_explicit_references_is_accepted() {
    let prefix = STUB_TOKEN_PREFIXES[0];
    assert_eq!(
        parse(&format!("${{{prefix}name}}")).unwrap(),
        TokenReference::EnvVar(format!("{prefix}name"))
    );
    assert_eq!(
        parse(&format!("env:{prefix}name")).unwrap(),
        TokenReference::EnvVar(format!("{prefix}name"))
    );
    assert_eq!(
        parse(&format!("keyring:{prefix}name")).unwrap(),
        TokenReference::Keyring(format!("{prefix}name"))
    );
}

#[test]
fn test_prefixes_come_only_from_the_registry() {
    let value = format!("{}xxxxxxxx", STUB_TOKEN_PREFIXES[0]);
    let result = TokenReference::parse(&value, &PluginRegistry::new());
    assert!(matches!(result, Err(TokenRefError::InvalidFormat(_))));
}

#[test]
fn test_reject_long_generic_token_without_any_prefix() {
    let result = TokenReference::parse(
        "0123456789abcdef0123456789abcdef0123",
        &PluginRegistry::new(),
    );
    assert!(matches!(result, Err(TokenRefError::PlainTextToken(_))));
}
