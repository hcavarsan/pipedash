use std::collections::HashMap;

use pipedash_plugin_api::{
    ComponentFilter,
    FeedFormat,
    PluginError,
    PluginResult,
    StatusSource,
};

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, String, String)> {
    let parts: Vec<&str> = id.split("__").collect();

    if parts.len() < 4 || parts[0] != "bitbucket" {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: {}",
            id
        )));
    }

    let provider_id = parts[1]
        .parse::<i64>()
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid provider ID in: {}", id)))?;

    let workspace = parts[2].to_string();
    let repo_slug = parts[3].to_string();

    Ok((provider_id, workspace, repo_slug))
}

pub(crate) fn check_step_uuid(id: &str) -> PluginResult<()> {
    let is_uuid = id
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .is_some_and(|uuid| {
            uuid.len() == 36
                && uuid.bytes().enumerate().all(|(index, byte)| match index {
                    8 | 13 | 18 | 23 => byte == b'-',
                    _ => byte.is_ascii_hexdigit(),
                })
        });
    if is_uuid {
        Ok(())
    } else {
        Err(PluginError::InvalidConfig(format!(
            "Invalid Bitbucket step ID: {id}"
        )))
    }
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

pub(crate) fn get_api_url() -> String {
    "https://api.bitbucket.org/2.0".to_string()
}

pub(crate) fn get_auth(config: &HashMap<String, String>) -> PluginResult<(String, String)> {
    let email = config
        .get("email")
        .ok_or_else(|| PluginError::InvalidConfig("Missing Atlassian account email".to_string()))?
        .clone();

    let api_token = config
        .get("api_token")
        .ok_or_else(|| PluginError::InvalidConfig("Missing Bitbucket API token".to_string()))?
        .clone();

    Ok((email, api_token))
}

pub(crate) fn status_source() -> PluginResult<StatusSource> {
    let url = |value: &str| {
        reqwest::Url::parse(value)
            .map_err(|e| PluginError::Internal(format!("Invalid Bitbucket status URL: {e}")))
    };
    Ok(StatusSource {
        name: "Bitbucket".to_string(),
        page_url: url("https://bitbucket.status.atlassian.com/")?,
        feed_url: url("https://bitbucket.status.atlassian.com/api/v2/summary.json")?,
        format: FeedFormat::Statuspage,
        components: ComponentFilter::Ids(
            [
                "k0x2yw1435v7",
                "rzh2n85q7vtt",
                "rfzky0v13fbp",
                "c1qmcrcbc5zy",
            ]
            .map(str::to_string)
            .to_vec(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_source_is_the_bitbucket_statuspage_feed() {
        let source = status_source().unwrap();
        assert_eq!(source.feed_url.path(), "/api/v2/summary.json");
        assert_eq!(source.format, FeedFormat::Statuspage);
        assert!(matches!(source.components, ComponentFilter::Ids(ids) if ids.len() == 4));
    }
}
