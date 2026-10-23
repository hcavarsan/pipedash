use std::collections::HashMap;

use pipedash_plugin_api::{
    custom_status_source,
    ComponentFilter,
    FeedFormat,
    PluginError,
    PluginResult,
    StatusSource,
};

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, i64)> {
    let parts: Vec<&str> = id.split("__").collect();

    if parts.len() < 3 || parts[0] != "gitlab" {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: {}",
            id
        )));
    }

    let provider_id = parts[1]
        .parse::<i64>()
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid provider ID in: {}", id)))?;

    let project_id = parts[2]
        .parse::<i64>()
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid project ID in: {}", id)))?;

    Ok((provider_id, project_id))
}

pub(crate) fn parse_selected_items(config: &HashMap<String, String>) -> Option<Vec<String>> {
    config.get("selected_items").map(|items| {
        items
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
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
        .unwrap_or_else(|| "https://gitlab.com".to_string())
}

pub(crate) fn build_api_url(base_url: &str) -> String {
    format!("{}/api/v4", base_url)
}

pub(crate) fn status_source(
    config: &HashMap<String, String>,
) -> PluginResult<Option<StatusSource>> {
    if let Some(custom) = custom_status_source(config)? {
        return Ok(Some(custom));
    }
    let is_saas = reqwest::Url::parse(&get_base_url(config))
        .is_ok_and(|url| url.host_str() == Some("gitlab.com"));
    if !is_saas {
        return Ok(None);
    }
    let url = |value: &str| {
        reqwest::Url::parse(value)
            .map_err(|e| PluginError::Internal(format!("Invalid GitLab status URL: {e}")))
    };
    Ok(Some(StatusSource {
        name: "GitLab".to_string(),
        page_url: url("https://status.gitlab.com/")?,
        feed_url: url("https://api.status.io/1.0/status/5b36dc6502d06804c08349f7")?,
        format: FeedFormat::StatusIo,
        components: ComponentFilter::Ids(
            [
                "5b36e05f02d06804c0834a09",
                "5d2f74932676bc45e4927ead",
                "67ea346990c050053ab18e89",
                "5b36e06c633e9004b3d624ad",
                "5e29c1403f4deb04c0d7f035",
                "650c87fbadb04b052efa2bc9",
            ]
            .map(str::to_string)
            .to_vec(),
        ),
    }))
}

#[cfg(test)]
mod tests {
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
}
