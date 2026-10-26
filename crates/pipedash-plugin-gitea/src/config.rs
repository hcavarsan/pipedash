use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> PluginResult<String> {
    let url = config
        .get("base_url")
        .map(|url| url.trim().trim_end_matches('/'))
        .filter(|url| !url.is_empty())
        .ok_or_else(|| PluginError::InvalidConfig("Missing Gitea base URL".to_string()))?;
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

pub(crate) fn get_repositories(config: &HashMap<String, String>) -> Vec<String> {
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
    pub provider_id: i64,
    pub owner: &'a str,
    pub repo: &'a str,
    pub workflow: &'a str,
}

pub(crate) fn build_pipeline_id(
    provider_id: i64, owner: &str, repo: &str, workflow: &str,
) -> String {
    format!("gitea__{provider_id}__{owner}/{repo}/{workflow}")
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<PipelineId<'_>> {
    let invalid = || PluginError::InvalidConfig(format!("Invalid pipeline ID format: {id}"));

    let mut head = id.splitn(3, "__");
    let (Some("gitea"), Some(provider_id), Some(rest)) = (head.next(), head.next(), head.next())
    else {
        return Err(invalid());
    };
    let provider_id = provider_id.parse().map_err(|_| invalid())?;

    let mut tail = rest.splitn(3, '/');
    let (Some(owner), Some(repo), Some(workflow)) = (tail.next(), tail.next(), tail.next()) else {
        return Err(invalid());
    };
    if owner.is_empty() || repo.is_empty() || workflow.is_empty() {
        return Err(invalid());
    }

    Ok(PipelineId {
        provider_id,
        owner,
        repo,
        workflow,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_id_round_trips_names_containing_double_underscores() {
        let id = build_pipeline_id(7, "a__b", "my__repo", "release__v2.yaml");
        assert_eq!(
            parse_pipeline_id(&id).unwrap(),
            PipelineId {
                provider_id: 7,
                owner: "a__b",
                repo: "my__repo",
                workflow: "release__v2.yaml",
            }
        );
    }

    #[test]
    fn pipeline_id_rejects_malformed_input() {
        for id in [
            "github__1__o/r/w.yaml",
            "gitea__x__o/r/w.yaml",
            "gitea__1__o/r",
            "gitea__1__/r/w.yaml",
            "gitea__1",
        ] {
            assert!(parse_pipeline_id(id).is_err(), "{id}");
        }
    }

    #[test]
    fn base_url_is_required_and_trimmed() {
        let config = HashMap::from([(
            "base_url".to_string(),
            " https://git.example.com/ ".to_string(),
        )]);
        assert_eq!(get_base_url(&config).unwrap(), "https://git.example.com");
        assert!(get_base_url(&HashMap::new()).is_err());
    }

    #[cfg(not(target_os = "ios"))]
    #[test]
    fn desktop_and_web_keep_supporting_self_hosted_http() {
        let config = HashMap::from([(
            "base_url".to_string(),
            " http://git.example.com/ ".to_string(),
        )]);
        assert_eq!(get_base_url(&config).unwrap(), "http://git.example.com");
    }

    #[cfg(target_os = "ios")]
    #[test]
    fn ios_requires_an_https_server_url() {
        for base_url in [
            "http://git.example.com",
            "git.example.com",
            "not a URL",
            "https://",
        ] {
            let config = HashMap::from([("base_url".to_string(), base_url.to_string())]);
            let error = get_base_url(&config).unwrap_err();
            assert!(matches!(error, PluginError::InvalidConfig(_)));
            assert!(error.to_string().contains("HTTPS"));
        }
    }

    #[cfg(target_os = "ios")]
    #[test]
    fn ios_initialization_checks_the_url_before_missing_credentials() {
        use pipedash_plugin_api::Plugin;

        let config =
            HashMap::from([("base_url".to_string(), "http://git.example.com".to_string())]);
        let error = crate::GiteaPlugin::new()
            .initialize(1, config, None)
            .unwrap_err();
        assert!(matches!(error, PluginError::InvalidConfig(_)));
        assert!(error.to_string().contains("HTTPS"));
    }

    #[test]
    fn parse_repo_requires_owner_and_name() {
        assert_eq!(parse_repo("acme/web").unwrap(), ("acme", "web"));
        assert!(parse_repo("web").is_err());
        assert!(parse_repo("acme/web/extra").is_err());
    }
}
