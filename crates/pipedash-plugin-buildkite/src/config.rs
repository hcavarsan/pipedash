use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn parse_selected_items(
    config: &HashMap<String, String>,
) -> PluginResult<(String, Vec<String>)> {
    let selected_items = config
        .get("selected_items")
        .or_else(|| config.get("pipelines")) // Fallback for backward compatibility
        .ok_or_else(|| PluginError::InvalidConfig("No pipelines selected".to_string()))?;

    let organizations: HashMap<String, i32> = HashMap::new();
    let mut pipeline_slugs = Vec::new();

    for item in selected_items.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() == 2 {
            let _org = parts[0].to_string();
            let slug = parts[1].to_string();

            pipeline_slugs.push(slug);
        }
    }

    let organization = organizations
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(org, _)| org)
        .or_else(|| config.get("organization").cloned()) // Fallback
        .ok_or_else(|| {
            PluginError::InvalidConfig("Could not determine organization".to_string())
        })?;

    Ok((organization, pipeline_slugs))
}

pub(crate) fn parse_repository_name(repo_url: &str) -> String {
    let cleaned = repo_url.trim_end_matches(".git").trim_end_matches('/');

    if let Some(pos) = cleaned.rfind(':') {
        let after_colon = &cleaned[pos + 1..];
        if after_colon.contains('/') {
            return after_colon.to_string();
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
    }
}
