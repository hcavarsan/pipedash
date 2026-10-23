use std::collections::HashMap;

use pipedash_plugin_api::{
    custom_status_source,
    ComponentFilter,
    FeedFormat,
    PluginError,
    PluginResult,
    StatusSource,
};
use reqwest::Url;

const GITHUB_STATUS_COMPONENTS: [&str; 4] = [
    "br0l2tvcx85d",
    "brv1bkgrwx7q",
    "8l4ygp009s5s",
    "4230lsnqdsld",
];

pub(crate) fn get_repositories(config: &HashMap<String, String>) -> Vec<String> {
    config
        .get("selected_items")
        .or_else(|| config.get("repositories")) // Fallback for backward compatibility
        .map(|items| {
            items
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_repo(repo: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> String {
    config
        .get("base_url")
        .and_then(|url| {
            let trimmed = url.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.trim_end_matches('/').to_string())
            }
        })
        .unwrap_or_else(|| "https://github.com".to_string())
}

fn parse_base_url(base_url: &str) -> PluginResult<Url> {
    let url = Url::parse(base_url)
        .map_err(|e| PluginError::InvalidConfig(format!("Invalid base_url '{base_url}': {e}")))?;
    if url.host_str().is_none() {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid base_url '{base_url}': missing host"
        )));
    }
    Ok(url)
}

fn is_github_com(host: &str) -> bool {
    matches!(host, "github.com" | "www.github.com" | "api.github.com")
}

pub(crate) fn build_api_url(base_url: &str) -> PluginResult<String> {
    let url = parse_base_url(base_url)?;
    let base = base_url.trim_end_matches('/');

    Ok(match url.host_str().unwrap_or_default() {
        "github.com" | "www.github.com" => "https://api.github.com".to_string(),
        "api.github.com" => base.to_string(),
        host if host.ends_with(".ghe.com") => format!("https://api.{host}"),
        _ => format!("{base}/api/v3"),
    })
}

pub(crate) fn status_source(
    config: &HashMap<String, String>, base_url: &str,
) -> PluginResult<Option<StatusSource>> {
    if let Some(source) = custom_status_source(config)? {
        return Ok(Some(source));
    }

    if !is_github_com(parse_base_url(base_url)?.host_str().unwrap_or_default()) {
        return Ok(None);
    }

    let url = |raw: &str| {
        Url::parse(raw).map_err(|e| PluginError::Internal(format!("Invalid status URL: {e}")))
    };
    Ok(Some(StatusSource {
        name: "GitHub".to_string(),
        page_url: url("https://www.githubstatus.com/")?,
        feed_url: url("https://www.githubstatus.com/api/v2/summary.json")?,
        format: FeedFormat::Statuspage,
        components: ComponentFilter::Ids(
            GITHUB_STATUS_COMPONENTS
                .iter()
                .map(|id| id.to_string())
                .collect(),
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_repo() {
        assert_eq!(
            parse_repo("owner/repo"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(parse_repo("invalid"), None);
        assert_eq!(parse_repo("owner/repo/extra"), None);
    }

    #[test]
    fn test_get_base_url() {
        let mut config = HashMap::new();

        assert_eq!(get_base_url(&config), "https://github.com");

        config.insert(
            "base_url".to_string(),
            "https://github.enterprise.com".to_string(),
        );
        assert_eq!(get_base_url(&config), "https://github.enterprise.com");

        config.insert(
            "base_url".to_string(),
            "https://github.enterprise.com/".to_string(),
        );
        assert_eq!(get_base_url(&config), "https://github.enterprise.com");

        config.insert("base_url".to_string(), "  ".to_string());
        assert_eq!(get_base_url(&config), "https://github.com");
    }

    fn api_url(base_url: &str) -> String {
        build_api_url(base_url).unwrap()
    }

    #[test]
    fn github_com_hosts_route_to_the_public_api() {
        assert_eq!(api_url("https://github.com"), "https://api.github.com");
        assert_eq!(api_url("https://github.com/"), "https://api.github.com");
        assert_eq!(api_url("https://www.github.com"), "https://api.github.com");
        assert_eq!(api_url("https://api.github.com"), "https://api.github.com");
        assert_eq!(api_url("https://api.github.com/"), "https://api.github.com");
    }

    #[test]
    fn ghe_com_subdomain_routes_to_its_api_host() {
        assert_eq!(api_url("https://acme.ghe.com"), "https://api.acme.ghe.com");
        assert_eq!(api_url("https://acme.ghe.com/"), "https://api.acme.ghe.com");
    }

    #[test]
    fn look_alike_hosts_stay_on_their_own_server() {
        assert_eq!(
            api_url("https://github.com.corp.net"),
            "https://github.com.corp.net/api/v3"
        );
        assert_eq!(
            api_url("https://github.company.com"),
            "https://github.company.com/api/v3"
        );
        assert_eq!(
            api_url("https://notgithub.com"),
            "https://notgithub.com/api/v3"
        );
    }

    #[test]
    fn ghes_appends_api_v3() {
        assert_eq!(
            api_url("https://github.enterprise.com"),
            "https://github.enterprise.com/api/v3"
        );
        assert_eq!(
            api_url("https://github.enterprise.com/"),
            "https://github.enterprise.com/api/v3"
        );
        assert_eq!(
            api_url("http://127.0.0.1:8080"),
            "http://127.0.0.1:8080/api/v3"
        );
    }

    #[test]
    fn unparsable_base_url_is_invalid_config() {
        assert!(matches!(
            build_api_url("not a url"),
            Err(PluginError::InvalidConfig(_))
        ));
    }

    fn source_for(base_url: &str, extra: &[(&str, &str)]) -> PluginResult<Option<StatusSource>> {
        let config = extra
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        status_source(&config, base_url)
    }

    #[test]
    fn github_com_gets_the_githubstatus_source() {
        for base_url in [
            "https://github.com",
            "https://www.github.com",
            "https://api.github.com",
        ] {
            let source = source_for(base_url, &[]).unwrap().unwrap();
            assert_eq!(source.name, "GitHub");
            assert_eq!(source.page_url.as_str(), "https://www.githubstatus.com/");
            assert_eq!(
                source.feed_url.as_str(),
                "https://www.githubstatus.com/api/v2/summary.json"
            );
            assert_eq!(source.format, FeedFormat::Statuspage);
            assert_eq!(
                source.components,
                ComponentFilter::Ids(vec![
                    "br0l2tvcx85d".to_string(),
                    "brv1bkgrwx7q".to_string(),
                    "8l4ygp009s5s".to_string(),
                    "4230lsnqdsld".to_string(),
                ])
            );
        }
    }

    #[test]
    fn other_hosts_have_no_source_without_a_custom_page() {
        for base_url in [
            "https://github.enterprise.com",
            "https://github.com.corp.net",
            "https://acme.ghe.com",
        ] {
            assert_eq!(source_for(base_url, &[]).unwrap(), None);
        }
    }

    #[test]
    fn custom_status_page_wins_over_the_github_com_default() {
        let source = source_for(
            "https://github.com",
            &[("status_page_url", "https://status.example.com")],
        )
        .unwrap()
        .unwrap();

        assert_eq!(source.name, "status.example.com");
        assert_eq!(source.components, ComponentFilter::All);
        assert_eq!(
            source.feed_url.as_str(),
            "https://status.example.com/api/v2/summary.json"
        );
    }

    #[test]
    fn invalid_custom_status_page_is_invalid_config() {
        let error = source_for(
            "https://github.com",
            &[("status_page_url", "http://10.0.0.1")],
        )
        .unwrap_err();

        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error}");
    }
}
