use std::collections::HashMap;

use super::*;
use crate::application::test_support::plugins::{
    local_plugin,
    manifest,
    registry,
    STUB,
    STUB_LOCAL,
    STUB_TOKEN_PREFIXES,
};

fn check(config: &PipedashConfig) -> ValidationResult {
    ConfigValidator::validate_with_plugins(config, &registry())
}

fn provider(provider_type: &str, token: &str) -> ProviderFileConfig {
    ProviderFileConfig {
        name: None,
        provider_type: provider_type.to_string(),
        token: token.to_string(),
        refresh_interval: 30,
        config: HashMap::new(),
    }
}

fn unknown_type_warnings(result: &ValidationResult) -> usize {
    result
        .warnings
        .iter()
        .filter(|w| w.message.starts_with("Unknown provider type"))
        .count()
}

#[test]
fn test_valid_config() {
    let config = PipedashConfig::default();
    let result = check(&config);
    assert!(result.is_ok());
}

#[test]
fn test_postgres_missing_connection_string() {
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackend::Postgres;

    let result = check(&config);
    assert!(!result.is_ok());
    assert!(result
        .errors
        .iter()
        .any(|e| e.field.contains("postgres.connection_string")));
}

#[test]
fn test_provider_missing_type() {
    let mut config = PipedashConfig::default();
    config
        .providers
        .insert("test".to_string(), provider("", ""));

    let result = check(&config);
    assert!(result
        .errors
        .iter()
        .any(|e| e.field.contains(".type") && e.code == ConfigErrorCode::MissingRequired));
    assert_eq!(unknown_type_warnings(&result), 0);
}

#[test]
fn validate_views_rejects_a_view_without_rules() {
    let mut config = PipedashConfig::default();
    config.views.insert(
        "release".to_string(),
        ViewFileConfig {
            name: "Release".to_string(),
            ..Default::default()
        },
    );
    config.views.insert(
        "broken".to_string(),
        ViewFileConfig {
            name: "Broken".to_string(),
            repositories: vec!["acme/[".to_string()],
            ..Default::default()
        },
    );

    let result = check(&config);

    assert!(result
        .errors
        .iter()
        .any(|e| e.field == "views.release" && e.code == ConfigErrorCode::MissingRequired));
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.field == "views.broken.repositories"
                && e.code == ConfigErrorCode::InvalidValue)
    );
}

#[test]
fn test_provider_any_token_format_allowed() {
    let mut config = PipedashConfig::default();
    let token = format!("{}xxxxxxxxxxxxxxxxxxxxxxxxxxxx", STUB_TOKEN_PREFIXES[0]);
    config
        .providers
        .insert("test".to_string(), provider(STUB, &token));

    let result = check(&config);
    assert!(!result
        .errors
        .iter()
        .any(|e| e.code == ConfigErrorCode::PlainTextToken));
}

#[test]
fn test_provider_no_token_warning() {
    let mut config = PipedashConfig::default();
    config
        .providers
        .insert("test".to_string(), provider(STUB, ""));

    let result = check(&config);
    assert!(result
        .warnings
        .iter()
        .any(|w| w.code == ConfigWarningCode::NoTokenConfigured));
}

#[test]
fn registered_provider_types_do_not_warn() {
    let mut config = PipedashConfig::default();
    config
        .providers
        .insert("test".to_string(), provider(STUB, "${STUB_TOKEN}"));

    assert_eq!(unknown_type_warnings(&check(&config)), 0);
}

#[test]
fn unknown_provider_type_warns_and_lists_registered_types() {
    let mut config = PipedashConfig::default();
    config
        .providers
        .insert("test".to_string(), provider("not-registered", ""));

    let result = check(&config);
    let warning = result
        .warnings
        .iter()
        .find(|w| w.message.starts_with("Unknown provider type"))
        .unwrap();
    assert_eq!(warning.field, "providers.test.type");
    assert_eq!(warning.code, ConfigWarningCode::UnusedSetting);
    assert!(warning.message.contains(&format!("\"{STUB}\"")));
    assert!(result.is_ok());
}

#[test]
fn registering_a_plugin_makes_its_type_valid() {
    let mut config = PipedashConfig::default();
    config
        .providers
        .insert("test".to_string(), provider(STUB_LOCAL, ""));

    let mut local = PluginRegistry::new();
    assert_eq!(
        unknown_type_warnings(&ConfigValidator::validate_with_plugins(&config, &local)),
        1
    );

    local.register(local_plugin, manifest(STUB_LOCAL));
    assert_eq!(
        unknown_type_warnings(&ConfigValidator::validate_with_plugins(&config, &local)),
        0
    );
}

fn with_providers(application_toml: &str) -> String {
    format!("[providers.ci]\ntype = \"stub\"\n\n{application_toml}")
}

fn application_errors(toml: &str) -> Vec<String> {
    let config =
        crate::infrastructure::config::loader::ConfigLoader::parse(&with_providers(toml)).unwrap();
    check(&config)
        .errors
        .into_iter()
        .filter(|e| e.field.starts_with("applications") || e.field.starts_with("correlation"))
        .map(|e| format!("{}: {}", e.field, e.message))
        .collect()
}

const MEMBER: &str = "[[applications.x.members]]\nprovider = \"ci\"\npipeline = \"a-*\"\n";

#[test]
fn valid_application_has_no_errors() {
    let errors = application_errors(&format!("[applications.x]\nname = \"X\"\n{MEMBER}"));
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn unknown_provider_key_is_an_error() {
    let errors = application_errors(
        "[applications.x]\nname = \"X\"\n[[applications.x.members]]\nprovider = \"nope\"\npipeline = \"a\"\n",
    );
    assert!(
        errors.iter().any(|e| e.contains("members[0].provider")),
        "{errors:?}"
    );
}

#[test]
fn unknown_stage_is_a_parse_error() {
    let content = with_providers(
        "[applications.x]\nname = \"X\"\n[[applications.x.members]]\nprovider = \"ci\"\npipeline = \"a\"\nstage = \"sideways\"\n",
    );
    assert!(crate::infrastructure::config::loader::ConfigLoader::parse(&content).is_err());
}

#[test]
fn cycle_in_after_is_an_error() {
    let errors = application_errors(&format!(
        "[applications.x]\nname = \"X\"\n{MEMBER}[applications.x.targets.a]\nname = \"A\"\nafter = [\"b\"]\n[applications.x.targets.b]\nname = \"B\"\nafter = [\"a\"]\n"
    ));
    assert!(errors.iter().any(|e| e.contains("cycle")), "{errors:?}");
}

#[test]
fn unknown_after_target_is_an_error() {
    let errors = application_errors(&format!(
        "[applications.x]\nname = \"X\"\n{MEMBER}[applications.x.targets.a]\nname = \"A\"\nafter = [\"ghost\"]\n"
    ));
    assert!(
        errors.iter().any(|e| e.contains("Unknown target")),
        "{errors:?}"
    );
}

#[test]
fn empty_members_is_an_error() {
    let errors = application_errors("[applications.x]\nname = \"X\"\n");
    assert!(
        errors.iter().any(|e| e.contains("applications.x.members")),
        "{errors:?}"
    );
}

#[test]
fn invalid_glob_is_an_error() {
    let errors = application_errors(
        "[applications.x]\nname = \"X\"\n[[applications.x.members]]\nprovider = \"ci\"\npipeline = \"[unclosed\"\n",
    );
    assert!(
        errors.iter().any(|e| e.contains("Invalid pattern")),
        "{errors:?}"
    );
}
