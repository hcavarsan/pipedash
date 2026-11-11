use serde_json::json;

use super::*;

fn field(key: &str) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: key.to_string(),
        description: None,
        field_type: ConfigFieldType::Text,
        required: false,
        default_value: None,
        options: None,
        validation_regex: None,
        validation_message: None,
        section: ConfigSection::Connection,
        auth_modes: Vec::new(),
    }
}

#[test]
fn auth_options_and_field_modes_round_trip() {
    let schema = ConfigSchema::new()
        .auth_option(AuthOption::none())
        .auth_option(AuthOption::kubeconfig("Uses a kubeconfig"))
        .add_field(field("deck_url"))
        .add_auth_field(&[AuthMode::Kubeconfig], field("context"));
    let value = serde_json::to_value(&schema).unwrap();

    assert_eq!(
        value["auth_options"][1],
        json!({"mode": "kubeconfig", "label": "Kubernetes", "description": "Uses a kubeconfig"})
    );
    assert!(value["fields"][0].get("auth_modes").is_none());
    assert_eq!(value["fields"][1]["auth_modes"], json!(["kubeconfig"]));

    let back: ConfigSchema = serde_json::from_value(value).unwrap();
    assert_eq!(back.auth_options, schema.auth_options);
    assert_eq!(back.fields[1].auth_modes, vec![AuthMode::Kubeconfig]);
    assert_eq!(back.default_mode(), AuthMode::None);
}

#[test]
fn a_schema_without_options_is_token_only() {
    let schema = ConfigSchema::new().add_field(field("token"));

    assert!(serde_json::to_value(&schema)
        .unwrap()
        .get("auth_options")
        .is_none());
    assert_eq!(schema.default_mode(), AuthMode::Token);
    assert!(schema.allows(AuthMode::Token));
    assert!(!schema.allows(AuthMode::None));
}

#[test]
fn each_mode_sees_only_its_own_field_for_a_shared_key() {
    let mut pasted = field("token");
    pasted.label = "pasted".to_string();
    let schema = ConfigSchema::new()
        .auth_option(AuthOption::token("Token", "t"))
        .auth_option(AuthOption::kubeconfig("k"))
        .add_field(field("name"))
        .add_auth_field(&[AuthMode::Token], field("token"))
        .add_auth_field(&[AuthMode::Kubeconfig], pasted);
    let labels = |mode| {
        schema
            .active_fields(mode)
            .map(|field| field.label.as_str())
            .collect::<Vec<_>>()
    };

    assert_eq!(labels(AuthMode::Token), ["name", "token"]);
    assert_eq!(labels(AuthMode::Kubeconfig), ["name", "pasted"]);
    assert_eq!(labels(AuthMode::None), ["name"]);
    assert!(!schema.allows(AuthMode::None));
}

#[test]
fn auth_mode_is_parsed_from_the_config() {
    let config = |mode: &str| HashMap::from([(AUTH_MODE_KEY.to_string(), mode.to_string())]);

    for mode in [
        AuthMode::None,
        AuthMode::Token,
        AuthMode::SignIn,
        AuthMode::Kubeconfig,
    ] {
        assert_eq!(AuthMode::parse(mode.as_str()), Some(mode));
    }
    assert_eq!(
        AuthMode::from_config(&config(" kubeconfig ")),
        Some(AuthMode::Kubeconfig)
    );
    assert_eq!(AuthMode::from_config(&config("Token")), None);
    assert_eq!(AuthMode::from_config(&config("oauth")), None);
    assert_eq!(AuthMode::from_config(&HashMap::new()), None);
    assert!(is_no_auth(&config("none")));
    assert!(!is_no_auth(&config("token")));
    assert!(!is_no_auth(&HashMap::new()));
}

#[test]
fn fields_serialize_their_section_in_snake_case() {
    let mut auth = field("token");
    auth.section = ConfigSection::Authentication;
    assert_eq!(
        serde_json::to_value(&auth).unwrap()["section"],
        json!("authentication")
    );
}
