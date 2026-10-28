use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use reqwest::Url;

pub(crate) const PROVIDER_TYPE: &str = "azure-devops";

pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<Url> {
    let raw = config
        .get("server_url")
        .map(|url| url.trim())
        .filter(|url| !url.is_empty())
        .ok_or_else(|| PluginError::InvalidConfig("Missing Azure DevOps server URL".to_string()))?;
    let mut url = Url::parse(raw)
        .map_err(|e| PluginError::InvalidConfig(format!("Invalid Azure DevOps server URL: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(PluginError::InvalidConfig(
            "Azure DevOps server URL must start with http:// or https://".to_string(),
        ));
    }
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

pub(crate) fn get_projects(config: &HashMap<String, String>) -> Vec<String> {
    config
        .get("selected_items")
        .map(|items| {
            items
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PipelineId<'a> {
    pub project: &'a str,
    pub definition_id: i64,
}

pub(crate) fn build_pipeline_id(provider_id: i64, project: &str, definition_id: i64) -> String {
    format!("{PROVIDER_TYPE}__{provider_id}__{project}/{definition_id}")
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<PipelineId<'_>> {
    let invalid = || PluginError::InvalidConfig(format!("Invalid pipeline ID format: {id}"));

    let mut head = id.splitn(3, "__");
    let (Some(PROVIDER_TYPE), Some(provider_id), Some(rest)) =
        (head.next(), head.next(), head.next())
    else {
        return Err(invalid());
    };
    provider_id.parse::<i64>().map_err(|_| invalid())?;

    let (project, definition_id) = rest.rsplit_once('/').ok_or_else(invalid)?;
    if project.is_empty() {
        return Err(invalid());
    }

    Ok(PipelineId {
        project,
        definition_id: definition_id.parse().map_err(|_| invalid())?,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(url: &str) -> HashMap<String, String> {
        HashMap::from([("server_url".to_string(), url.to_string())])
    }

    #[test]
    fn pipeline_id_round_trips_project_names_with_spaces_and_double_underscores() {
        let id = build_pipeline_id(7, "My__Project Name", 42);
        assert_eq!(id, "azure-devops__7__My__Project Name/42");
        assert_eq!(
            parse_pipeline_id(&id).unwrap(),
            PipelineId {
                project: "My__Project Name",
                definition_id: 42,
            }
        );
    }

    #[test]
    fn pipeline_id_rejects_malformed_input() {
        for id in [
            "azuredevops__1__proj/5",
            "github__1__proj/5",
            "azure-devops__x__proj/5",
            "azure-devops__1__proj",
            "azure-devops__1__proj/five",
            "azure-devops__1__/5",
            "azure-devops__1",
        ] {
            assert!(parse_pipeline_id(id).is_err(), "{id}");
        }
    }

    #[test]
    fn server_url_is_required_trimmed_and_stripped_of_query_and_fragment() {
        let url = get_server_url(&config_with(" https://dev.azure.com/acme/?a=1#top ")).unwrap();
        assert_eq!(url.as_str(), "https://dev.azure.com/acme/");
        assert!(get_server_url(&HashMap::new()).is_err());
        assert!(get_server_url(&config_with("   ")).is_err());
    }

    #[test]
    fn server_url_must_be_http_or_https() {
        for url in ["dev.azure.com/acme", "ftp://dev.azure.com/acme", "https://"] {
            let error = get_server_url(&config_with(url)).unwrap_err();
            assert!(matches!(error, PluginError::InvalidConfig(_)), "{url}");
        }
    }

    #[test]
    fn organization_is_the_last_path_segment_of_the_server_url() {
        for (url, organization) in [
            ("https://dev.azure.com/acme", "acme"),
            ("https://dev.azure.com/acme/", "acme"),
            (
                "https://tfs.example.com/tfs/DefaultCollection/",
                "DefaultCollection",
            ),
            ("https://tfs.example.com", "tfs.example.com"),
        ] {
            let url = get_server_url(&config_with(url)).unwrap();
            assert_eq!(organization_of(&url), organization, "{url}");
        }
    }

    #[test]
    fn selected_projects_are_split_and_trimmed() {
        let config = HashMap::from([(
            "selected_items".to_string(),
            " Web App, ,Mobile ".to_string(),
        )]);
        assert_eq!(get_projects(&config), ["Web App", "Mobile"]);
        assert!(get_projects(&HashMap::new()).is_empty());
    }

    #[test]
    fn web_url_encodes_the_project_and_never_doubles_the_slash() {
        let server = get_server_url(&config_with("https://dev.azure.com/acme/")).unwrap();
        assert_eq!(
            build_web_url(&server, "My Project", 9),
            "https://dev.azure.com/acme/My%20Project/_build/results?buildId=9"
        );
    }
}
