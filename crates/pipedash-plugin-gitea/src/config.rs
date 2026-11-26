use std::collections::HashMap;

use pipedash_plugin_api::config::{
    base_url,
    required_base_url,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    RepoId,
};

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> PluginResult<String> {
    let url = required_base_url(config, "base_url", "Missing Gitea base URL")?;
    #[cfg(target_os = "ios")]
    if !reqwest::Url::parse(url)
        .is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
    {
        return Err(PluginError::InvalidConfig(
            "Gitea on iOS requires an HTTPS server URL, such as https://git.example.com. Enable HTTPS on your server before adding or importing this provider."
                .to_string(),
        ));
    }
    Ok(url.to_string())
}

pub(crate) fn source_host(config: &HashMap<String, String>) -> Option<String> {
    let url = base_url(config, "base_url")?;
    RepoId::parse(&format!("{url}/_")).map(|repo| repo.host().to_string())
}

pub(crate) fn source_hosts(config: &HashMap<String, String>) -> Vec<String> {
    source_host(config).into_iter().collect()
}

pub(crate) fn parse_repo(full_name: &str) -> PluginResult<(&str, &str)> {
    full_name
        .split_once('/')
        .filter(|(owner, repo)| !owner.is_empty() && !repo.is_empty() && !repo.contains('/'))
        .ok_or_else(|| {
            PluginError::InvalidConfig(format!("Invalid repository format: {full_name}"))
        })
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PipelineId<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub workflow: &'a str,
}

pub(crate) fn build_pipeline_id(owner: &str, repo: &str, workflow: &str) -> String {
    format!("{owner}/{repo}/{workflow}")
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<PipelineId<'_>> {
    let invalid = || {
        PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: '{{owner}}/{{repo}}/{{workflow}}'"
        ))
    };
    let mut tail = id.splitn(3, '/');
    let (Some(owner), Some(repo), Some(workflow)) = (tail.next(), tail.next(), tail.next()) else {
        return Err(invalid());
    };
    if owner.is_empty() || repo.is_empty() || workflow.is_empty() {
        return Err(invalid());
    }

    Ok(PipelineId {
        owner,
        repo,
        workflow,
    })
}

#[cfg(test)]
mod tests;
