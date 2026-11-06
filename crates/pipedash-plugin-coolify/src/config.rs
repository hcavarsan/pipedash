use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use reqwest::Url;

const PIPELINE_ID_PREFIX: &str = "coolify";
const PIPELINE_ID_SEPARATOR: &str = "__";

pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<String> {
    let url = config
        .get("server_url")
        .map(|url| url.trim().trim_end_matches('/'))
        .filter(|url| !url.is_empty())
        .ok_or_else(|| PluginError::InvalidConfig("Missing server_url in config".to_string()))?;
    let invalid =
        |reason: &str| PluginError::InvalidConfig(format!("Invalid Coolify URL '{url}': {reason}"));
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
    Ok(url.to_string())
}

pub(crate) fn get_credential(config: &HashMap<String, String>, key: &str) -> PluginResult<String> {
    config
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| PluginError::InvalidConfig(format!("Missing {key} in config")))
}

pub(crate) fn is_insecure(config: &HashMap<String, String>) -> bool {
    config
        .get("insecure")
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(false)
}

pub(crate) fn parse_selected_items(config: &HashMap<String, String>) -> Option<Vec<String>> {
    config.get("selected_items").map(|items| {
        items
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    })
}

pub(crate) fn build_pipeline_id(provider_id: i64, application_uuid: &str) -> String {
    format!(
        "{PIPELINE_ID_PREFIX}{PIPELINE_ID_SEPARATOR}{provider_id}{PIPELINE_ID_SEPARATOR}{application_uuid}"
    )
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, &str)> {
    let invalid = || {
        PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: 'coolify__{{provider_id}}__{{application_uuid}}'"
        ))
    };
    let mut parts = id.splitn(3, PIPELINE_ID_SEPARATOR);
    let (Some(PIPELINE_ID_PREFIX), Some(provider_id), Some(application_uuid)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    let provider_id = provider_id.parse::<i64>().map_err(|_| invalid())?;
    if application_uuid.is_empty() {
        return Err(invalid());
    }

    Ok((provider_id, application_uuid))
}

#[cfg(test)]
mod tests {
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
            "https://coolify.example.com?x=1",
            "https://coolify.example.com#top",
            "file:///etc/passwd",
        ] {
            assert!(
                matches!(url(raw), Err(PluginError::InvalidConfig(_))),
                "{raw}"
            );
        }
    }

    #[test]
    fn pipeline_ids_round_trip() {
        let id = build_pipeline_id(7, "ygow8swkc8ko08wocw0so8wk");

        assert_eq!(id, "coolify__7__ygow8swkc8ko08wocw0so8wk");
        assert_eq!(
            parse_pipeline_id(&id).unwrap(),
            (7, "ygow8swkc8ko08wocw0so8wk")
        );
    }

    #[test]
    fn malformed_pipeline_ids_are_rejected() {
        for id in [
            "",
            "coolify",
            "coolify__7",
            "coolify__7__",
            "coolify__x__a",
            "komodo__7__a",
        ] {
            assert!(
                matches!(parse_pipeline_id(id), Err(PluginError::InvalidConfig(_))),
                "{id}"
            );
        }
    }
}
