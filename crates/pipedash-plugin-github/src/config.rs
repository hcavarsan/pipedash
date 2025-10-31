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
}
