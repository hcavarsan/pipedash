use super::*;
use crate::test_env::EnvGuard;

#[test]
fn test_simple_var() {
    std::env::set_var("TEST_VAR_SIMPLE", "hello");
    let result = interpolate("${TEST_VAR_SIMPLE}").unwrap();
    assert_eq!(result, "hello");
    std::env::remove_var("TEST_VAR_SIMPLE");
}

#[test]
fn test_var_with_text() {
    std::env::set_var("TEST_VAR_TEXT", "world");
    let result = interpolate("Hello ${TEST_VAR_TEXT}!").unwrap();
    assert_eq!(result, "Hello world!");
    std::env::remove_var("TEST_VAR_TEXT");
}

#[test]
fn test_multiple_vars() {
    std::env::set_var("TEST_VAR_A", "foo");
    std::env::set_var("TEST_VAR_B", "bar");
    let result = interpolate("${TEST_VAR_A}-${TEST_VAR_B}").unwrap();
    assert_eq!(result, "foo-bar");
    std::env::remove_var("TEST_VAR_A");
    std::env::remove_var("TEST_VAR_B");
}

#[test]
fn test_missing_var_error() {
    let result = interpolate("${THIS_VAR_DOES_NOT_EXIST_12345}");
    assert!(matches!(
        result,
        Err(InterpolationError::RequiredVarNotFound(_))
    ));
}

#[test]
fn test_default_value() {
    let result = interpolate("${NONEXISTENT_VAR_123:-default_value}").unwrap();
    assert_eq!(result, "default_value");
}

#[test]
fn test_empty_default() {
    let result = interpolate("prefix${NONEXISTENT_VAR_456:-}suffix").unwrap();
    assert_eq!(result, "prefixsuffix");
}

#[test]
fn test_var_overrides_default() {
    std::env::set_var("TEST_VAR_OVERRIDE", "actual");
    let result = interpolate("${TEST_VAR_OVERRIDE:-default}").unwrap();
    assert_eq!(result, "actual");
    std::env::remove_var("TEST_VAR_OVERRIDE");
}

#[test]
fn test_nested_default() {
    std::env::set_var("TEST_NESTED_INNER", "inner_value");
    let result = interpolate("${NONEXISTENT:-${TEST_NESTED_INNER}}").unwrap();
    assert_eq!(result, "inner_value");
    std::env::remove_var("TEST_NESTED_INNER");
}

#[test]
fn test_no_interpolation() {
    let result = interpolate("plain text").unwrap();
    assert_eq!(result, "plain text");
}

#[test]
fn test_has_variables() {
    assert!(has_variables("${VAR}"));
    assert!(has_variables("prefix${VAR}suffix"));
    assert!(has_variables("${VAR:-default}"));
    assert!(!has_variables("plain text"));
    assert!(!has_variables("$VAR")); // Missing braces
}

#[test]
fn test_interpolate_toml() {
    std::env::set_var("TEST_TOML_VAR", "toml_value");

    let toml_str = r#"
        key = "${TEST_TOML_VAR}"
        nested = { inner = "${TEST_TOML_VAR:-fallback}" }
        array = ["${TEST_TOML_VAR}", "static"]
    "#;

    let mut value: toml::Value = toml::from_str(toml_str).unwrap();
    interpolate_toml(&mut value).unwrap();

    assert_eq!(value["key"].as_str().unwrap(), "toml_value");
    assert_eq!(value["nested"]["inner"].as_str().unwrap(), "toml_value");
    assert_eq!(value["array"][0].as_str().unwrap(), "toml_value");
    assert_eq!(value["array"][1].as_str().unwrap(), "static");

    std::env::remove_var("TEST_TOML_VAR");
}

#[test]
fn test_complex_address() {
    std::env::set_var("TEST_HOST", "localhost");
    std::env::set_var("TEST_PORT", "9000");

    let result = interpolate("${TEST_HOST:-127.0.0.1}:${TEST_PORT:-8080}").unwrap();
    assert_eq!(result, "localhost:9000");

    std::env::remove_var("TEST_HOST");
    std::env::remove_var("TEST_PORT");

    let result = interpolate("${TEST_HOST:-127.0.0.1}:${TEST_PORT:-8080}").unwrap();
    assert_eq!(result, "127.0.0.1:8080");
}

#[test]
fn test_env_values_are_inserted_verbatim() {
    std::env::set_var("TEST_VERBATIM_SET", "expanded");
    std::env::set_var("TEST_VERBATIM_A", "tok${TEST_VERBATIM_SET}en");
    std::env::set_var("TEST_VERBATIM_B", "pa${TEST_VERBATIM_UNSET}ss");
    std::env::set_var("TEST_VERBATIM_SELF", "x${TEST_VERBATIM_SELF}");

    assert_eq!(
        interpolate("${TEST_VERBATIM_A}").unwrap(),
        "tok${TEST_VERBATIM_SET}en"
    );
    assert_eq!(
        interpolate("${TEST_VERBATIM_B}").unwrap(),
        "pa${TEST_VERBATIM_UNSET}ss"
    );
    assert_eq!(
        interpolate("${TEST_VERBATIM_SELF}").unwrap(),
        "x${TEST_VERBATIM_SELF}"
    );
}

#[test]
fn test_file_fallback_when_var_unset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("postgres_url");
    std::fs::write(
        &path,
        "postgres://app:pa${NOT_EXPANDED}ss@db/pipedash\r\n\n",
    )
    .unwrap();
    let _env = EnvGuard::set(&[
        ("TEST_SECRET_URL", None),
        ("TEST_SECRET_URL_FILE", path.to_str()),
    ]);

    assert_eq!(
        interpolate("${TEST_SECRET_URL:-sqlite}").unwrap(),
        "postgres://app:pa${NOT_EXPANDED}ss@db/pipedash"
    );
}

#[test]
fn test_env_var_wins_over_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token");
    std::fs::write(&path, "from-file\n").unwrap();
    let _env = EnvGuard::set(&[
        ("TEST_SECRET_TOKEN", Some("from-env")),
        ("TEST_SECRET_TOKEN_FILE", path.to_str()),
    ]);

    assert_eq!(interpolate("${TEST_SECRET_TOKEN}").unwrap(), "from-env");
}

#[test]
fn test_unreadable_file_fails_instead_of_default() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    let _env = EnvGuard::set(&[
        ("TEST_SECRET_MISSING", None),
        ("TEST_SECRET_MISSING_FILE", missing.to_str()),
    ]);

    let err = interpolate("${TEST_SECRET_MISSING:-fallback}").unwrap_err();
    assert!(matches!(
        &err,
        InterpolationError::SecretFile { var, path, .. }
            if var == "TEST_SECRET_MISSING_FILE" && path == &missing
    ));
}
