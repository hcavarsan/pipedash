use super::*;

#[test]
fn selected_items_carry_their_organization() {
    let config: HashMap<String, String> = serde_json::from_str(
        r#"{"token":"test","selected_items":"acme/backend,acme/web,other/deploy"}"#,
    )
    .unwrap();
    assert_eq!(
        parse_selected_items(&config).unwrap(),
        vec![
            ("acme".to_string(), "backend".to_string()),
            ("acme".to_string(), "web".to_string()),
            ("other".to_string(), "deploy".to_string()),
        ]
    );
}

#[test]
fn example_config_pipelines_are_kept() {
    let config: HashMap<String, String> = serde_json::from_str(
        r#"{"organization":"mycompany","selected_items":"deploy-pipeline,test-pipeline"}"#,
    )
    .unwrap();
    assert_eq!(
        parse_selected_items(&config).unwrap(),
        vec![
            ("mycompany".to_string(), "deploy-pipeline".to_string()),
            ("mycompany".to_string(), "test-pipeline".to_string()),
        ]
    );
}

#[test]
fn bare_slug_without_any_organization_is_rejected() {
    let config: HashMap<String, String> =
        serde_json::from_str(r#"{"selected_items":"deploy-pipeline"}"#).unwrap();
    assert!(parse_selected_items(&config).is_err());
}
