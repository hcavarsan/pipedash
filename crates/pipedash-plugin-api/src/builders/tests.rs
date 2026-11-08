use serde_json::json;

use super::*;

#[test]
fn token_field_is_a_required_password_in_authentication() {
    let field = serde_json::to_value(ConfigField::token("API Token", "Needs read scope")).unwrap();
    assert_eq!(
        field,
        json!({
            "key": "token",
            "label": "API Token",
            "description": "Needs read scope",
            "field_type": "Password",
            "required": true,
            "default_value": null,
            "options": null,
            "validation_regex": null,
            "validation_message": null,
            "section": "authentication",
        })
    );
}

#[test]
fn insecure_tls_field_defaults_to_false() {
    let field = ConfigField::insecure_tls();
    assert_eq!(field.key, "insecure");
    assert_eq!(field.default_value, Some(json!(false)));
    assert!(!field.required);
}

#[test]
fn select_and_setters_fill_the_optional_members() {
    let field = ConfigField::select("mode", ConfigSection::Settings, "Mode", &["a", "b"])
        .default_text("a")
        .http_url_validation()
        .required();
    assert_eq!(field.options, Some(vec!["a".to_string(), "b".to_string()]));
    assert_eq!(field.default_value, Some(json!("a")));
    assert_eq!(field.validation_regex.as_deref(), Some(HTTP_URL_REGEX));
    assert_eq!(field.validation_message.as_deref(), Some(HTTP_URL_MESSAGE));
    assert!(field.required);
    assert_eq!(
        ConfigField::select("context", ConfigSection::Authentication, "Context", &[]).options,
        Some(Vec::new())
    );
}

#[test]
fn standard_capabilities_run_and_log_pipelines() {
    let capabilities = PluginCapabilities::standard();
    assert!(capabilities.trigger && capabilities.cancel && capabilities.logs);
    assert!(!capabilities.force_cancel && !capabilities.sign_in);
    assert!(capabilities.uses_http_proxy && !capabilities.async_trigger);
}

#[test]
fn metadata_new_leaves_manifest_fields_to_the_registry() {
    let metadata = PluginMetadata::new(
        "Example",
        ConfigSchema::new(),
        TableSchema::new(),
        PluginCapabilities::standard(),
    )
    .with_permissions(vec![Permission::new("read", "Read things", true)])
    .with_features(vec![Feature::new("view", "View", "View things", &["read"])]);
    assert_eq!(metadata.icon, None);
    assert_eq!(metadata.author, None);
    assert!(metadata.provider_type.is_empty() && metadata.version.is_empty());
    assert!(metadata.description.is_empty());
    assert_eq!(metadata.required_permissions.len(), 1);
    assert_eq!(metadata.features[0].required_permissions, vec!["read"]);
    assert_eq!(metadata.run_history, RunHistoryPolicy::default());
    assert!(metadata.token_prefixes.is_empty());
    assert_eq!(metadata.presentation, PluginPresentation::new("Example"));
}
