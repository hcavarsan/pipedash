use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

/// Parse ArgoCD pipeline ID format:
/// argocd__{provider_id}__{namespace}__{app_name}
pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, String, String)> {
    let parts: Vec<&str> = id.split("__").collect();

    if parts.len() != 4 || parts[0] != "argocd" {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{}'. Expected format: 'argocd__{{provider_id}}__{{namespace}}__{{app_name}}'",
            id
        )));
    }

    let provider_id = parts[1].parse::<i64>().map_err(|_| {
        PluginError::InvalidConfig(format!(
            "Invalid provider ID '{}' in pipeline ID '{}'. Provider ID must be a valid integer",
            parts[1], id
        ))
    })?;

    let namespace = parts[2].to_string();
    let app_name = parts[3].to_string();

    Ok((provider_id, namespace, app_name))
}

/// Get server URL from config, ensuring it doesn't have a trailing slash
pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<String> {
    config
        .get("server_url")
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .ok_or_else(|| PluginError::InvalidConfig("Missing server_url in config".to_string()))
}

/// Get authentication token from config
pub(crate) fn get_token(config: &HashMap<String, String>) -> PluginResult<String> {
    config
        .get("token")
        .map(|t| t.trim().to_string())
        .ok_or_else(|| PluginError::InvalidConfig("Missing token in config".to_string()))
}

/// Check if insecure TLS verification should be skipped
pub(crate) fn is_insecure(config: &HashMap<String, String>) -> bool {
    config
        .get("insecure")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false)
}

/// Parse Git organizations filter from config (comma-separated list)
pub(crate) fn parse_organizations_filter(config: &HashMap<String, String>) -> Option<Vec<String>> {
    config.get("organizations").map(|orgs| {
        orgs.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

/// Generate pipeline ID from components
pub(crate) fn build_pipeline_id(provider_id: i64, namespace: &str, app_name: &str) -> String {
    format!("argocd__{}__{}__{}", provider_id, namespace, app_name)
}

/// Parse selected application names from config (comma-separated)
pub(crate) fn parse_selected_items(config: &HashMap<String, String>) -> Option<Vec<String>> {
    config.get("selected_items").map(|items| {
        items
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

/// Parse Git repository URL into "org/repo" format
/// Handles both HTTPS and SSH URL formats
/// Examples:
/// - https://github.com/org/repo.git -> org/repo
/// - git@github.com:org/repo.git -> org/repo
/// - https://gitlab.com/group/subgroup/repo -> subgroup/repo
pub(crate) fn parse_repository_name(repo_url: &str) -> String {
    let cleaned = repo_url.trim_end_matches(".git").trim_end_matches('/');

    // For SSH format (git@host:org/repo)
    // But skip if it's a URL protocol (contains ://)
    if let Some(pos) = cleaned.rfind(':') {
        let after_colon = &cleaned[pos + 1..];
        // Only treat as SSH if it doesn't start with '//' (which indicates URL
        // protocol)
        if after_colon.contains('/') && !after_colon.starts_with("//") {
            return after_colon.to_string();
        }
    }

    // For HTTPS format (https://host/org/repo or https://host/org/subgroup/repo)
    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() >= 2 {
        let org = parts[parts.len() - 2];
        let repo = parts[parts.len() - 1];
        return format!("{}/{}", org, repo);
    }

    // Fallback: return as-is
    repo_url.to_string()
}

/// Extract Git organization from repository URL
/// Returns the first part of the repository name (org from org/repo)
pub(crate) fn extract_git_org(repo_url: &str) -> String {
    let repo_name = parse_repository_name(repo_url);
    repo_name.split('/').next().unwrap_or("unknown").to_string()
}
