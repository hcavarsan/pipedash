use std::collections::HashMap;

use pipedash_plugin_api::config::{
    invalid_pipeline_id,
    PIPELINE_ID_SEPARATOR,
};
use pipedash_plugin_api::{
    ComponentFilter,
    FeedFormat,
    PluginError,
    PluginResult,
    RepoId,
    StatusSource,
};

const HOST: &str = "bitbucket.org";

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(String, String)> {
    let (workspace, rest) = id
        .split_once(PIPELINE_ID_SEPARATOR)
        .ok_or_else(|| invalid_pipeline_id(id))?;
    let repo_slug = rest.split(PIPELINE_ID_SEPARATOR).next().unwrap_or(rest);
    Ok((workspace.to_string(), repo_slug.to_string()))
}

pub(crate) fn parse_repo(repo: &RepoId) -> PluginResult<(&str, &str)> {
    repo.path()
        .split_once('/')
        .filter(|(workspace, slug)| {
            repo.host() == HOST && !workspace.is_empty() && !slug.is_empty() && !slug.contains('/')
        })
        .ok_or_else(|| {
            PluginError::InvalidConfig(format!(
                "Not a Bitbucket Cloud repository: {}",
                repo.as_str()
            ))
        })
}

pub(crate) fn repo_id(workspace: &str, repo_slug: &str) -> Option<RepoId> {
    RepoId::from_host_path(HOST, &format!("{workspace}/{repo_slug}"))
}

pub(crate) fn source_hosts(config: &HashMap<String, String>) -> Vec<String> {
    if config.is_empty() {
        Vec::new()
    } else {
        vec![HOST.to_string()]
    }
}

pub(crate) fn check_step_uuid(id: &str) -> PluginResult<()> {
    let is_uuid = id
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .is_some_and(|uuid| {
            uuid.len() == 36
                && uuid.bytes().enumerate().all(|(index, byte)| match index {
                    8 | 13 | 18 | 23 => byte == b'-',
                    _ => byte.is_ascii_hexdigit(),
                })
        });
    if is_uuid {
        Ok(())
    } else {
        Err(PluginError::InvalidConfig(format!(
            "Invalid Bitbucket step ID: {id}"
        )))
    }
}

pub(crate) fn get_api_url() -> String {
    "https://api.bitbucket.org/2.0".to_string()
}

pub(crate) fn get_auth(config: &HashMap<String, String>) -> PluginResult<(String, String)> {
    let email = config
        .get("email")
        .ok_or_else(|| PluginError::InvalidConfig("Missing Atlassian account email".to_string()))?
        .clone();

    let api_token = config
        .get("api_token")
        .ok_or_else(|| PluginError::InvalidConfig("Missing Bitbucket API token".to_string()))?
        .clone();

    Ok((email, api_token))
}

pub(crate) fn status_source() -> PluginResult<StatusSource> {
    let url = |value: &str| {
        reqwest::Url::parse(value)
            .map_err(|e| PluginError::Internal(format!("Invalid Bitbucket status URL: {e}")))
    };
    Ok(StatusSource {
        name: "Bitbucket".to_string(),
        page_url: url("https://bitbucket.status.atlassian.com/")?,
        feed_url: url("https://bitbucket.status.atlassian.com/api/v2/summary.json")?,
        format: FeedFormat::Statuspage,
        components: ComponentFilter::Ids(
            [
                "k0x2yw1435v7",
                "rzh2n85q7vtt",
                "rfzky0v13fbp",
                "c1qmcrcbc5zy",
            ]
            .map(str::to_string)
            .to_vec(),
        ),
    })
}

#[cfg(test)]
mod tests;
