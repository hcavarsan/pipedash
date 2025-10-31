//! Configuration parsing and validation for Buildkite plugin

use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

/// Parses the selected items from config to extract organization and pipeline
/// slugs
pub(crate) fn parse_selected_items(
    config: &HashMap<String, String>,
) -> PluginResult<(String, Vec<String>)> {
    // Parse selected_items which contains "org/pipeline-slug" format IDs
    let selected_items = config
        .get("selected_items")
        .or_else(|| config.get("pipelines")) // Fallback for backward compatibility
        .ok_or_else(|| PluginError::InvalidConfig("No pipelines selected".to_string()))?;

    let mut organizations = HashMap::new();
    let mut pipeline_slugs = Vec::new();

    for item in selected_items.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse "org/slug" format
        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() == 2 {
            let org = parts[0].to_string();
            let slug = parts[1].to_string();

            *organizations.entry(org.clone()).or_insert(0) += 1;
            pipeline_slugs.push(slug);
        }
    }

    // Use the most common organization (or first one)
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

/// Parses various git URL formats to extract org/repo
///
/// # Examples
///
/// - `https://github.com/org/repo.git` -> `org/repo`
/// - `git@github.com:org/repo.git` -> `org/repo`
/// - `https://github.com/org/repo` -> `org/repo`
pub(crate) fn parse_repository_name(repo_url: &str) -> String {
    let cleaned = repo_url.trim_end_matches(".git").trim_end_matches('/');

    // For SSH format (git@host:org/repo)
    if let Some(pos) = cleaned.rfind(':') {
        let after_colon = &cleaned[pos + 1..];
        if after_colon.contains('/') {
            return after_colon.to_string();
        }
    }

    // For HTTPS format (https://host/org/repo)
    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() >= 2 {
        let org = parts[parts.len() - 2];
        let repo = parts[parts.len() - 1];
        return format!("{org}/{repo}");
    }

    // Fallback: return as-is
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
