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
    }
}

#[test]
fn auth_fields_serialize_their_modes_by_key() {
    let schema = ConfigSchema::new()
        .allow_no_auth()
        .add_field(field("deck_url"))
        .add_auth_field(&[AuthMode::SignIn], field("sso_issuer"))
        .add_auth_field(&[AuthMode::Token, AuthMode::SignIn], field("kubeconfig"));
    let value = serde_json::to_value(&schema).unwrap();
    assert_eq!(
        value["auth_modes"],
        json!({"sso_issuer": ["sign_in"], "kubeconfig": ["token", "sign_in"]})
    );
    assert_eq!(value["no_auth"], json!(true));
    assert_eq!(value["fields"].as_array().unwrap().len(), 3);

    let plain = serde_json::to_value(ConfigSchema::new()).unwrap();
    assert!(plain.get("auth_modes").is_none());
    assert!(plain.get("no_auth").is_none());
}

#[test]
fn no_auth_is_read_from_the_auth_mode_key() {
    let config = |mode: &str| HashMap::from([(AUTH_MODE_KEY.to_string(), mode.to_string())]);
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
