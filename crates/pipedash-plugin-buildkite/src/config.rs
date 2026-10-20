use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn parse_selected_items(
    config: &HashMap<String, String>,
) -> PluginResult<Vec<(String, String)>> {
    let selected_items = config
        .get("selected_items")
        .or_else(|| config.get("pipelines")) // Fallback for backward compatibility
        .ok_or_else(|| PluginError::InvalidConfig("No pipelines selected".to_string()))?;

    let mut pipelines = Vec::new();

    for item in selected_items.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split('/').collect();
        match parts.as_slice() {
            [org, slug] => pipelines.push((org.to_string(), slug.to_string())),
            [slug] => {
                let org = config.get("organization").cloned().ok_or_else(|| {
                    PluginError::InvalidConfig(format!(
                        "Could not determine organization of pipeline {slug}"
                    ))
                })?;
                pipelines.push((org, slug.to_string()));
            }
            _ => {}
        }
    }

    Ok(pipelines)
}

pub(crate) fn parse_repository_name(repo_url: &str) -> String {
    let cleaned = repo_url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/');

    if !cleaned.contains("://") {
        if let Some(pos) = cleaned.rfind(':') {
            let after_colon = &cleaned[pos + 1..];
            if after_colon.contains('/') {
                return after_colon.to_string();
            }
        }
    }

    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() >= 2 {
        let org = parts[parts.len() - 2];
        let repo = parts[parts.len() - 1];
        return format!("{org}/{repo}");
    }

    repo_url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_repository_name() {
        assert_eq!(
            parse_repository_name("https://github.com/org/repo.git"),
            "org/repo"
        );
        assert_eq!(
            parse_repository_name("git@github.com:org/repo.git"),
            "org/repo"
        );
        assert_eq!(
            parse_repository_name("https://github.com/org/repo"),
            "org/repo"
        );
        assert_eq!(
            parse_repository_name("https://github.com/org/repo.git/"),
            "org/repo"
        );
        assert_eq!(
            parse_repository_name("ssh://git@github.com/org/repo.git"),
            "org/repo"
        );
        assert_eq!(
            parse_repository_name("git@github.com:org/repo/"),
            "org/repo"
        );
    }

    #[test]
    fn selected_items_carry_their_organization() {
        let config: HashMap<String, String> = serde_json::from_str(
            r#"{"token":"test","selected_items":"acme/backend,acme/web,other/deploy"}"#,
        )
        .unwrap();
        assert_eq!(
            parse_selected_items(&config).unwrap(),
            vec![
                ("acme".to_string(), "backend".to_string()),
                ("acme".to_string(), "web".to_string()),
                ("other".to_string(), "deploy".to_string()),
            ]
        );
    }

    #[test]
    fn example_config_pipelines_are_kept() {
        let config: HashMap<String, String> = serde_json::from_str(
            r#"{"organization":"mycompany","selected_items":"deploy-pipeline,test-pipeline"}"#,
        )
        .unwrap();
        assert_eq!(
            parse_selected_items(&config).unwrap(),
            vec![
                ("mycompany".to_string(), "deploy-pipeline".to_string()),
                ("mycompany".to_string(), "test-pipeline".to_string()),
            ]
        );
    }

    #[test]
    fn bare_slug_without_any_organization_is_rejected() {
        let config: HashMap<String, String> =
            serde_json::from_str(r#"{"selected_items":"deploy-pipeline"}"#).unwrap();
        assert!(parse_selected_items(&config).is_err());
    }
}
