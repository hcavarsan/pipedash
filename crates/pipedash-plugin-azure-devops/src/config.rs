use std::collections::HashMap;

use pipedash_plugin_api::config::{
    invalid_pipeline_id,
    required_value,
};
use pipedash_plugin_api::http::parse_http_url;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
    RepoId,
};
use reqwest::Url;

pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<Url> {
    let raw = required_value(config, "server_url", "Missing Azure DevOps server URL")?;
    let mut url = parse_http_url(raw, "Azure DevOps server")?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub(crate) fn organization_of(server_url: &Url) -> String {
    server_url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .or_else(|| server_url.host_str())
        .unwrap_or_default()
        .to_string()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PipelineId<'a> {
    pub project: &'a str,
    pub definition_id: i64,
}

pub(crate) fn build_pipeline_id(project: &str, definition_id: i64) -> String {
    format!("{project}/{definition_id}")
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<PipelineId<'_>> {
    let (project, definition_id) = id
        .rsplit_once('/')
        .filter(|(project, _)| !project.is_empty())
        .ok_or_else(|| invalid_pipeline_id(id))?;

    Ok(PipelineId {
        project,
        definition_id: definition_id.parse().map_err(|_| invalid_pipeline_id(id))?,
    })
}

pub(crate) fn build_web_url(server_url: &Url, project: &str, build_id: i64) -> String {
    let mut url = server_url.clone();
    if let Ok(mut segments) = url.path_segments_mut() {
        segments
            .pop_if_empty()
            .push(project)
            .push("_build")
            .push("results");
    }
    url.query_pairs_mut()
        .append_pair("buildId", &build_id.to_string());
    url.to_string()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GitRepo {
    pub project: String,
    pub name: String,
}

pub(crate) fn git_repo(repo: &RepoId, organization: &str) -> PluginResult<GitRepo> {
    let foreign = || {
        PluginError::PipelineNotFound(format!(
            "{} is not a repository of the {organization} organization",
            repo.as_str()
        ))
    };
    let segments = repo
        .path()
        .split('/')
        .map(|segment| urlencoding::decode(segment).map(|decoded| decoded.into_owned()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| foreign())?;
    let (owner, rest) = segments.split_first().ok_or_else(foreign)?;
    if !owner.eq_ignore_ascii_case(organization) {
        return Err(foreign());
    }
    match rest {
        [project, marker, name] if marker == "_git" => Ok(GitRepo {
            project: project.clone(),
            name: name.clone(),
        }),
        [marker, name] if marker == "_git" => Ok(GitRepo {
            project: name.clone(),
            name: name.clone(),
        }),
        _ => Err(foreign()),
    }
}

#[cfg(test)]
mod tests;
