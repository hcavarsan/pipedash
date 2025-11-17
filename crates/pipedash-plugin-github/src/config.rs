//! Configuration parsing for GitHub plugin

use std::collections::HashMap;

/// Gets the list of repositories from configuration
pub(crate) fn get_repositories(config: &HashMap<String, String>) -> Vec<String> {
    // Parse selected_items which contains "owner/repo" format IDs
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

/// Parses a repository string into owner and name
///
/// # Example
///
/// ```ignore
/// let (owner, name) = parse_repo("owner/repo");
/// assert_eq!(owner, "owner");
/// assert_eq!(name, "repo");
/// ```
pub(crate) fn parse_repo(repo: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Gets the base URL from configuration, defaulting to GitHub.com
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

/// Builds the API URL from the base URL
pub(crate) fn build_api_url(base_url: &str) -> String {
    // GitHub Enterprise uses /api/v3, while GitHub.com uses api.github.com
    if base_url.contains("github.com") && !base_url.contains("api.github.com") {
        "https://api.github.com".to_string()
    } else {
        format!("{}/api/v3", base_url.trim_end_matches('/'))
    }
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

        // Default case
        assert_eq!(get_base_url(&config), "https://github.com");

        // Custom URL
        config.insert("base_url".to_string(), "https://github.enterprise.com".to_string());
        assert_eq!(get_base_url(&config), "https://github.enterprise.com");

        // Trim trailing slash
        config.insert("base_url".to_string(), "https://github.enterprise.com/".to_string());
        assert_eq!(get_base_url(&config), "https://github.enterprise.com");

        // Empty string should use default
        config.insert("base_url".to_string(), "  ".to_string());
        assert_eq!(get_base_url(&config), "https://github.com");
    }

    #[test]
    fn test_build_api_url() {
        // GitHub.com should use api.github.com
        assert_eq!(build_api_url("https://github.com"), "https://api.github.com");

        // Enterprise should append /api/v3
        assert_eq!(
            build_api_url("https://github.enterprise.com"),
            "https://github.enterprise.com/api/v3"
        );

        // Should handle trailing slash
        assert_eq!(
            build_api_url("https://github.enterprise.com/"),
            "https://github.enterprise.com/api/v3"
        );
    }
}
