use pipedash_plugin_api::{
    Plugin as PluginTrait,
    PluginRegistry,
};

macro_rules! define_plugins {
    ($(($provider_type:literal, $plugin_crate:ident :: $plugin_type:ident)),* $(,)?) => {
        pub fn init_registry() -> PluginRegistry {
            let mut registry = PluginRegistry::new();
            $(
                $plugin_crate::register(&mut registry);
            )*
            registry
        }

        pub fn create_instance(provider_type: &str) -> Option<Box<dyn PluginTrait>> {
            match provider_type {
                $(
                    $provider_type => Some(Box::new($plugin_crate::$plugin_type::new())),
                )*
                _ => None,
            }
        }
    };
}

define_plugins![
    ("github", pipedash_plugin_github::GitHubPlugin),
    ("buildkite", pipedash_plugin_buildkite::BuildkitePlugin),
    ("gitlab", pipedash_plugin_gitlab::GitLabPlugin),
    ("jenkins", pipedash_plugin_jenkins::JenkinsPlugin),
    ("tekton", pipedash_plugin_tekton::TektonPlugin),
    ("argocd", pipedash_plugin_argocd::ArgocdPlugin),
];
