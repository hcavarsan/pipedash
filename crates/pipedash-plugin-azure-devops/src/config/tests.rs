use pipedash_plugin_api::PluginError;

use super::*;

fn config_with(url: &str) -> HashMap<String, String> {
    HashMap::from([("server_url".to_string(), url.to_string())])
}

#[test]
fn pipeline_id_round_trips_project_names_with_spaces_and_double_underscores() {
    let id = build_pipeline_id("My__Project Name", 42);
    assert_eq!(id, "My__Project Name/42");
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
    for id in ["proj", "proj/five", "/5", ""] {
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
fn web_url_encodes_the_project_and_never_doubles_the_slash() {
    let server = get_server_url(&config_with("https://dev.azure.com/acme/")).unwrap();
    assert_eq!(
        build_web_url(&server, "My Project", 9),
        "https://dev.azure.com/acme/My%20Project/_build/results?buildId=9"
    );
}

fn repo(path: &str) -> RepoId {
    RepoId::from_host_path("dev.azure.com", path).unwrap()
}

#[test]
fn git_repo_decodes_the_project_and_repository_of_a_clone_url() {
    let git = |name: &str, project: &str| GitRepo {
        project: project.to_string(),
        name: name.to_string(),
    };
    assert_eq!(
        git_repo(&repo("acme/Web%20App/_git/shop"), "acme").unwrap(),
        git("shop", "Web App")
    );
    assert_eq!(
        git_repo(&repo("ACME/shop/_git/shop"), "acme").unwrap(),
        git("shop", "shop")
    );
    assert_eq!(
        git_repo(&repo("acme/_git/solo"), "acme").unwrap(),
        git("solo", "solo")
    );
}

#[test]
fn git_repo_rejects_other_organizations_and_non_git_paths() {
    for path in [
        "other/App/_git/shop",
        "acme/App/shop",
        "acme/App/_git",
        "acme",
        "acme/App/_git/shop/extra",
    ] {
        let error = git_repo(&repo(path), "acme").unwrap_err();
        assert!(matches!(error, PluginError::PipelineNotFound(_)), "{path}");
    }
}
