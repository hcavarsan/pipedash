use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn parse_selected_items(config: &HashMap<String, String>) -> PluginResult<Vec<String>> {
    let selected_items = config
        .get("selected_items")
        .ok_or_else(|| PluginError::InvalidConfig("No jobs selected".to_string()))?;

    Ok(selected_items
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

pub(crate) fn encode_job_name(name: &str) -> String {
    name.replace('/', "/job/")
}

pub(crate) fn split_job_path(job_path: &str) -> (String, String) {
    if let Some(slash_pos) = job_path.find('/') {
        let org = &job_path[..slash_pos];
        let repo = &job_path[slash_pos + 1..];
        (org.to_string(), repo.to_string())
    } else {
        (String::from("(root)"), job_path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_job_name() {
        assert_eq!(encode_job_name("folder/job"), "folder/job/job");
        assert_eq!(encode_job_name("simple"), "simple");
    }

    #[test]
    fn test_split_job_path() {
        assert_eq!(
            split_job_path("org/repo"),
            ("org".to_string(), "repo".to_string())
        );
        assert_eq!(
            split_job_path("simple"),
            ("(root)".to_string(), "simple".to_string())
        );
    }
}
