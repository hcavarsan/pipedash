use pipedash_plugin_api::{
    Plugin as PluginTrait,
    PluginMetadata,
    PluginRegistry,
};

pub fn create_plugin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();

    registry.register(Box::new(pipedash_plugin_github::GitHubPlugin::new()));
    registry.register(Box::new(pipedash_plugin_gitea::GiteaPlugin::new()));
    registry.register(Box::new(pipedash_plugin_gitlab::GitLabPlugin::new()));
    registry.register(Box::new(pipedash_plugin_bitbucket::BitbucketPlugin::new()));
    registry.register(Box::new(pipedash_plugin_buildkite::BuildkitePlugin::new()));
    registry.register(Box::new(
        pipedash_plugin_azure_devops::AzureDevOpsPlugin::new(),
    ));
    registry.register(Box::new(pipedash_plugin_jenkins::JenkinsPlugin::new()));
    registry.register(Box::new(pipedash_plugin_tekton::TektonPlugin::new()));
    registry.register(Box::new(pipedash_plugin_argocd::ArgocdPlugin::new()));
    registry.register(Box::new(pipedash_plugin_fluxcd::FluxcdPlugin::new()));

    registry
}

pub fn get_all_plugin_metadata() -> Vec<PluginMetadata> {
    let registry = create_plugin_registry();
    registry
        .provider_types()
        .iter()
        .filter_map(|provider_type| registry.get(provider_type))
        .map(|p| p.metadata().clone())
        .collect()
}

pub fn create_plugin(provider_type: &str) -> Option<Box<dyn PluginTrait>> {
    match provider_type {
        "github" => Some(Box::new(pipedash_plugin_github::GitHubPlugin::new())),
        "gitea" => Some(Box::new(pipedash_plugin_gitea::GiteaPlugin::new())),
        "gitlab" => Some(Box::new(pipedash_plugin_gitlab::GitLabPlugin::new())),
        "bitbucket" => Some(Box::new(pipedash_plugin_bitbucket::BitbucketPlugin::new())),
        "buildkite" => Some(Box::new(pipedash_plugin_buildkite::BuildkitePlugin::new())),
        "azure-devops" => Some(Box::new(
            pipedash_plugin_azure_devops::AzureDevOpsPlugin::new(),
        )),
        "jenkins" => Some(Box::new(pipedash_plugin_jenkins::JenkinsPlugin::new())),
        "tekton" => Some(Box::new(pipedash_plugin_tekton::TektonPlugin::new())),
        "argocd" => Some(Box::new(pipedash_plugin_argocd::ArgocdPlugin::new())),
        "fluxcd" => Some(Box::new(pipedash_plugin_fluxcd::FluxcdPlugin::new())),
        _ => None,
    }
}
