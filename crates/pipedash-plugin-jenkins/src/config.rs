use std::collections::HashMap;

use pipedash_plugin_api::config::comma_separated;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn follow_all_jobs(config: &HashMap<String, String>) -> bool {
    pipedash_plugin_api::config::bool_value(config, "follow_all_jobs")
}

pub(crate) fn parse_selected_items(config: &HashMap<String, String>) -> PluginResult<Vec<String>> {
    let selected_items = config
        .get("selected_items")
        .ok_or_else(|| PluginError::InvalidConfig("No jobs selected".to_string()))?;

    Ok(comma_separated(selected_items)
        .map(str::to_string)
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
mod tests;
