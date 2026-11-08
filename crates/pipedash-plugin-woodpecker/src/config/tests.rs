use pipedash_plugin_api::PluginError;

use super::*;

#[test]
fn pipeline_id_is_the_repository_id() {
    assert_eq!(parse_pipeline_id("3780").unwrap(), 3780);
}

#[test]
fn pipeline_id_rejects_malformed_input() {
    for id in ["", "x", "web", "5__6"] {
        assert!(parse_pipeline_id(id).is_err(), "{id}");
    }
}

#[cfg(not(target_os = "ios"))]
#[test]
fn base_url_is_a_plain_http_address() {
    let url = |raw: &str| get_base_url(&HashMap::from([("base_url".to_string(), raw.to_string())]));

    assert_eq!(url(" http://ci.local/ ").unwrap(), "http://ci.local");
    assert_eq!(
        url("https://example.com/woodpecker").unwrap(),
        "https://example.com/woodpecker"
    );
    assert!(get_base_url(&HashMap::new()).is_err());
    for raw in [
        "",
        "ci.example.com",
        "ftp://ci.example.com",
        "https://user:hunter2@ci.example.com?token=hunter2",
        "https://ci.example.com#top",
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
