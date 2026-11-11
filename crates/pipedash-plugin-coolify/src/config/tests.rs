use pipedash_plugin_api::PluginError;

use super::*;

#[test]
fn server_urls_must_be_plain_http_addresses() {
    let url = |raw: &str| {
        get_server_url(&HashMap::from([(
            "server_url".to_string(),
            raw.to_string(),
        )]))
    };

    assert_eq!(
        url(" https://coolify.example.com/ ").unwrap(),
        "https://coolify.example.com"
    );
    assert_eq!(
        url("http://10.0.0.5:8000/coolify").unwrap(),
        "http://10.0.0.5:8000/coolify"
    );
    for raw in [
        "",
        "coolify.example.com",
        "ftp://coolify.example.com",
        "https://user:hunter2@coolify.example.com?token=hunter2",
        "https://coolify.example.com#top",
        "file:///etc/passwd",
    ] {
        match url(raw) {
            Err(PluginError::InvalidConfig(message)) => {
                assert!(!message.contains("hunter2"), "{message}")
            }
            other => panic!("{raw}: {other:?}"),
        }
    }
}

#[test]
fn pipeline_ids_are_application_uuids() {
    assert_eq!(
        parse_pipeline_id("ygow8swkc8ko08wocw0so8wk").unwrap(),
        "ygow8swkc8ko08wocw0so8wk"
    );
}

#[test]
fn empty_pipeline_ids_are_rejected() {
    assert!(matches!(
        parse_pipeline_id(""),
        Err(PluginError::InvalidConfig(_))
    ));
}
