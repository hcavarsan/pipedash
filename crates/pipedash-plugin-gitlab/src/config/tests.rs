use pipedash_plugin_api::STATUS_PAGE_URL_KEY;

use super::*;

fn source_for(base_url: Option<&str>, status_page: Option<&str>) -> Option<StatusSource> {
    let mut config = HashMap::new();
    if let Some(value) = base_url {
        config.insert("base_url".to_string(), value.to_string());
    }
    if let Some(value) = status_page {
        config.insert(STATUS_PAGE_URL_KEY.to_string(), value.to_string());
    }
    status_source(&config).unwrap()
}

#[test]
fn saas_hosts_get_the_status_io_source() {
    for base_url in [
        None,
        Some(""),
        Some("https://gitlab.com"),
        Some("https://gitlab.com/"),
    ] {
        let source = source_for(base_url, None).unwrap();
        assert_eq!(source.name, "GitLab");
        assert_eq!(source.format, FeedFormat::StatusIo);
        assert_eq!(source.page_url.as_str(), "https://status.gitlab.com/");
        assert_eq!(
            source.feed_url.as_str(),
            "https://api.status.io/1.0/status/5b36dc6502d06804c08349f7"
        );
        assert!(matches!(&source.components, ComponentFilter::Ids(ids) if ids.len() == 6));
    }
}

#[test]
fn self_managed_and_lookalike_hosts_have_no_source() {
    for base_url in [
        "https://gitlab.example.com",
        "https://gitlab.com.evil.example",
        "https://evil.example/gitlab.com",
    ] {
        assert!(source_for(Some(base_url), None).is_none(), "{base_url}");
    }
}

#[test]
fn custom_status_page_wins_and_invalid_one_fails() {
    let source = source_for(
        Some("https://gitlab.com"),
        Some("https://status.example.com"),
    )
    .unwrap();
    assert_eq!(source.name, "status.example.com");
    assert_eq!(source.format, FeedFormat::Statuspage);
    assert!(source_for(
        Some("https://gitlab.example.com"),
        Some("https://s.example")
    )
    .is_some());

    let config = HashMap::from([(
        STATUS_PAGE_URL_KEY.to_string(),
        "javascript:alert(1)".to_string(),
    )]);
    assert!(matches!(
        status_source(&config),
        Err(PluginError::InvalidConfig(_))
    ));
}
