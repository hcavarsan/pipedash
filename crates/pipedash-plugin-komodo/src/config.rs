use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

const PIPELINE_ID_PREFIX: &str = "komodo";
const PIPELINE_ID_SEPARATOR: &str = "__";

pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<String> {
    config
        .get("server_url")
        .map(|url| url.trim().trim_end_matches('/'))
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .ok_or_else(|| PluginError::InvalidConfig("Missing server_url in config".to_string()))
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

pub(crate) fn build_pipeline_id(provider_id: i64, stack_id: &str) -> String {
    format!(
        "{PIPELINE_ID_PREFIX}{PIPELINE_ID_SEPARATOR}{provider_id}{PIPELINE_ID_SEPARATOR}{stack_id}"
    )
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, &str)> {
    let invalid = || {
        PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: 'komodo__{{provider_id}}__{{stack_id}}'"
        ))
    };
    let mut parts = id.splitn(3, PIPELINE_ID_SEPARATOR);
    let (Some(PIPELINE_ID_PREFIX), Some(provider_id), Some(stack_id)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    let provider_id = provider_id.parse::<i64>().map_err(|_| invalid())?;
    if stack_id.is_empty() {
        return Err(invalid());
    }

    Ok((provider_id, stack_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn server_url_drops_whitespace_and_trailing_slashes() {
        for raw in [
            "https://komodo.example.com",
            " https://komodo.example.com/ ",
            "https://komodo.example.com//",
        ] {
            assert_eq!(
                get_server_url(&cfg(&[("server_url", raw)])).unwrap(),
                "https://komodo.example.com"
            );
        }
        assert!(get_server_url(&cfg(&[("server_url", " / ")])).is_err());
        assert!(get_server_url(&cfg(&[])).is_err());
    }

    #[test]
    fn credentials_must_be_present_and_non_blank() {
        let config = cfg(&[("api_key", " K-1 "), ("token", "  ")]);

        assert_eq!(get_credential(&config, "api_key").unwrap(), "K-1");
        assert!(matches!(
            get_credential(&config, "token"),
            Err(PluginError::InvalidConfig(_))
        ));
        assert!(get_credential(&config, "missing").is_err());
    }

    #[test]
    fn insecure_is_true_only_for_the_word_true() {
        assert!(is_insecure(&cfg(&[("insecure", "true")])));
        for value in ["false", "yes", ""] {
            assert!(!is_insecure(&cfg(&[("insecure", value)])));
        }
        assert!(!is_insecure(&cfg(&[])));
    }

    #[test]
    fn selected_items_split_on_commas_and_keep_an_explicit_empty_selection() {
        assert_eq!(parse_selected_items(&cfg(&[])), None);
        assert_eq!(
            parse_selected_items(&cfg(&[("selected_items", " web, ,db ")])),
            Some(vec!["web".to_string(), "db".to_string()])
        );
        assert_eq!(
            parse_selected_items(&cfg(&[("selected_items", "")])),
            Some(Vec::new())
        );
    }

    #[test]
    fn pipeline_ids_round_trip() {
        let id = build_pipeline_id(7, "66a1b2c3d4e5f60718293a4b");

        assert_eq!(id, "komodo__7__66a1b2c3d4e5f60718293a4b");
        assert_eq!(
            parse_pipeline_id(&id).unwrap(),
            (7, "66a1b2c3d4e5f60718293a4b")
        );
    }

    #[test]
    fn malformed_pipeline_ids_are_rejected() {
        for id in [
            "",
            "komodo",
            "komodo__7",
            "komodo__7__",
            "komodo__x__stack",
            "argocd__7__stack",
        ] {
            assert!(
                matches!(parse_pipeline_id(id), Err(PluginError::InvalidConfig(_))),
                "{id}"
            );
        }
    }
}
