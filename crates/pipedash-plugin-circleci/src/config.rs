use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

use crate::client::validate_project_slug;

pub(crate) const DEFAULT_BASE_URL: &str = "https://circleci.com";
const PIPELINE_ID_PREFIX: &str = "circleci";
const PIPELINE_ID_SEPARATOR: &str = "__";

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> String {
    config
        .get("base_url")
        .map(|url| url.trim().trim_end_matches('/'))
        .filter(|url| !url.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .to_string()
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

pub(crate) fn build_pipeline_id(provider_id: i64, project_slug: &str) -> String {
    format!(
        "{PIPELINE_ID_PREFIX}{PIPELINE_ID_SEPARATOR}{provider_id}{PIPELINE_ID_SEPARATOR}{project_slug}"
    )
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, &str)> {
    let invalid = || {
        PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: 'circleci__{{provider_id}}__{{project_slug}}'"
        ))
    };
    let mut parts = id.splitn(3, PIPELINE_ID_SEPARATOR);
    let (Some(PIPELINE_ID_PREFIX), Some(provider_id), Some(project_slug)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    let provider_id = provider_id.parse::<i64>().map_err(|_| invalid())?;
    validate_project_slug(project_slug).map_err(|_| invalid())?;
    Ok((provider_id, project_slug))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_ids_round_trip_project_slugs() {
        for slug in [
            "gh/acme/web",
            "bb/acme/my__repo",
            "circleci/4Gl3lM5RtfFE7v5sa4nWAU/44n9wujWcTnVZ2b5S8Fnat",
        ] {
            let id = build_pipeline_id(7, slug);
            assert_eq!(parse_pipeline_id(&id).unwrap(), (7, slug));
        }
    }

    #[test]
    fn malformed_pipeline_ids_are_rejected() {
        for id in [
            "",
            "circleci",
            "circleci__7",
            "circleci__7__",
            "circleci__x__gh/acme/web",
            "circleci__7__gh/acme",
            "circleci__7__gh/acme/../web",
            "github__7__gh/acme/web",
        ] {
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
}
