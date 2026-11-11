use super::*;

#[test]
fn pipeline_id_round_trips_names_containing_double_underscores() {
    let id = build_pipeline_id("a__b", "my__repo", "release__v2.yaml");
    assert_eq!(
        parse_pipeline_id(&id).unwrap(),
        PipelineId {
            owner: "a__b",
            repo: "my__repo",
            workflow: "release__v2.yaml",
        }
    );
}

#[test]
fn pipeline_id_rejects_malformed_input() {
    for id in ["o/r", "/r/w.yaml", "o//w.yaml", "o/r/", "o"] {
        assert!(parse_pipeline_id(id).is_err(), "{id}");
    }
}

#[test]
fn base_url_is_required_and_trimmed() {
    let config = HashMap::from([(
        "base_url".to_string(),
        " https://git.example.com/ ".to_string(),
    )]);
    assert_eq!(get_base_url(&config).unwrap(), "https://git.example.com");
    assert!(get_base_url(&HashMap::new()).is_err());
}

#[cfg(not(target_os = "ios"))]
#[test]
fn desktop_and_web_keep_supporting_self_hosted_http() {
    let config = HashMap::from([(
        "base_url".to_string(),
        " http://git.example.com/ ".to_string(),
    )]);
    assert_eq!(get_base_url(&config).unwrap(), "http://git.example.com");
}

#[cfg(target_os = "ios")]
#[test]
fn ios_requires_an_https_server_url() {
    for base_url in [
        "http://git.example.com",
        "git.example.com",
        "not a URL",
        "https://",
    ] {
        let config = HashMap::from([("base_url".to_string(), base_url.to_string())]);
        let error = get_base_url(&config).unwrap_err();
        assert!(matches!(error, PluginError::InvalidConfig(_)));
        assert!(error.to_string().contains("HTTPS"));
    }
}

#[cfg(target_os = "ios")]
#[test]
fn ios_initialization_checks_the_url_before_missing_credentials() {
    use pipedash_plugin_api::Plugin;

    let config = HashMap::from([("base_url".to_string(), "http://git.example.com".to_string())]);
    let error = crate::GiteaPlugin::new()
        .initialize(1, config, None)
        .unwrap_err();
    assert!(matches!(error, PluginError::InvalidConfig(_)));
    assert!(error.to_string().contains("HTTPS"));
}

#[test]
fn parse_repo_requires_owner_and_name() {
    assert_eq!(parse_repo("acme/web").unwrap(), ("acme", "web"));
    assert!(parse_repo("web").is_err());
    assert!(parse_repo("acme/web/extra").is_err());
}
