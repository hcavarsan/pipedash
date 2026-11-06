use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use reqwest::Url;

pub(crate) const PROVIDER_TYPE: &str = "woodpecker";

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> PluginResult<String> {
    let url = config
        .get("base_url")
        .map(|url| url.trim().trim_end_matches('/'))
        .filter(|url| !url.is_empty())
        .ok_or_else(|| PluginError::InvalidConfig("Missing Woodpecker base URL".to_string()))?;
    let invalid =
        |reason: &str| PluginError::InvalidConfig(format!("Invalid Woodpecker URL: {reason}"));
    let parsed = Url::parse(url).map_err(|e| invalid(&e.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid("use http or https"));
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err(invalid("missing host"));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(invalid("remove the query or fragment"));
    }
    #[cfg(target_os = "ios")]
    if parsed.scheme() != "https" {
        return Err(PluginError::InvalidConfig(
            "Woodpecker on iOS requires an HTTPS server URL, such as https://ci.example.com. Enable HTTPS on your server before adding or importing this provider."
                .to_string(),
        ));
    }
    Ok(url.to_string())
}

pub(crate) fn get_repositories(config: &HashMap<String, String>) -> Vec<String> {
    config
        .get("selected_items")
        .map(|items| {
            items
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn build_pipeline_id(provider_id: i64, repo_id: i64) -> String {
    format!("{PROVIDER_TYPE}__{provider_id}__{repo_id}")
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<i64> {
    let invalid = || PluginError::InvalidConfig(format!("Invalid pipeline ID format: {id}"));

    let mut parts = id.splitn(3, "__");
    let (Some(PROVIDER_TYPE), Some(provider_id), Some(repo_id)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    provider_id.parse::<i64>().map_err(|_| invalid())?;
    repo_id.parse().map_err(|_| invalid())
}

pub(crate) fn pipeline_web_url(base_url: &str, repo_id: i64, number: i64) -> String {
    format!("{base_url}/repos/{repo_id}/pipeline/{number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_id_round_trips_the_repository_id() {
        let id = build_pipeline_id(7, 3780);
        assert_eq!(id, "woodpecker__7__3780");
        assert_eq!(parse_pipeline_id(&id).unwrap(), 3780);
    }

    #[test]
    fn pipeline_id_rejects_malformed_input() {
        for id in [
            "gitea__1__5",
            "woodpecker__x__5",
            "woodpecker__1__",
            "woodpecker__1__web",
            "woodpecker__1",
        ] {
            assert!(parse_pipeline_id(id).is_err(), "{id}");
        }
    }

    #[cfg(not(target_os = "ios"))]
    #[test]
    fn base_url_is_a_plain_http_address() {
        let url =
            |raw: &str| get_base_url(&HashMap::from([("base_url".to_string(), raw.to_string())]));

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
}
