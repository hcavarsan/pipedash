use pipedash_plugin_api::PluginError;

use super::*;

#[test]
fn pipeline_ids_are_project_slugs() {
    for slug in [
        "gh/acme/web",
        "bb/acme/my__repo",
        "circleci/4Gl3lM5RtfFE7v5sa4nWAU/44n9wujWcTnVZ2b5S8Fnat",
    ] {
        assert_eq!(parse_pipeline_id(slug).unwrap(), slug);
    }
}

#[test]
fn malformed_pipeline_ids_are_rejected() {
    for id in ["", "gh", "gh/acme", "gh/acme/../web", "gh/acme/web/extra"] {
        assert!(
            matches!(parse_pipeline_id(id), Err(PluginError::InvalidConfig(_))),
            "{id}"
        );
    }
}

#[test]
fn base_url_defaults_to_circleci_cloud() {
    let config = |value: &str| HashMap::from([("base_url".to_string(), value.to_string())]);

    assert_eq!(get_base_url(&HashMap::new()), DEFAULT_BASE_URL);
    assert_eq!(get_base_url(&config("  ")), DEFAULT_BASE_URL);
    assert_eq!(
        get_base_url(&config(" https://circleci.acme.dev/ ")),
        "https://circleci.acme.dev"
    );
}
