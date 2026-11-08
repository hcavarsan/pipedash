use std::collections::BTreeSet;
use std::path::PathBuf;

use kube::config::Kubeconfig;
use pipedash_plugin_api::kubernetes::KubeconfigSource;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

use crate::config;

pub(crate) fn parse_kubeconfig_content(content: &str) -> PluginResult<Kubeconfig> {
    Kubeconfig::from_yaml(content).map_err(|_| {
        PluginError::InvalidConfig(
            "The pasted kubeconfig content is not valid kubeconfig YAML".to_string(),
        )
    })
}

fn merge_kubeconfigs(paths: Vec<String>) -> PluginResult<Kubeconfig> {
    let mut merged = Kubeconfig::default();
    let mut found_any = false;

    for path in paths.into_iter().map(PathBuf::from) {
        if !path.exists() {
            continue;
        }
        let Ok(kubeconfig) = Kubeconfig::read_from(&path) else {
            continue;
        };
        found_any = true;
        merged.clusters.extend(kubeconfig.clusters);
        merged.auth_infos.extend(kubeconfig.auth_infos);
        merged.contexts.extend(kubeconfig.contexts);
        if merged.current_context.is_none() {
            merged.current_context = kubeconfig.current_context;
        }
    }

    if !found_any {
        return Err(PluginError::InvalidConfig(
            "No valid kubeconfig files found".to_string(),
        ));
    }

    Ok(merged)
}

pub(super) fn load_kubeconfig(
    source: &KubeconfigSource<Option<String>>,
) -> PluginResult<Kubeconfig> {
    match source {
        KubeconfigSource::Content(content) => parse_kubeconfig_content(content),
        KubeconfigSource::Path(path) => {
            let paths = match path {
                Some(path) => config::split_kubeconfig_paths(path),
                None => config::split_kubeconfig_paths(&config::get_default_kubeconfig_path()),
            };
            merge_kubeconfigs(paths)
        }
    }
}

pub(crate) async fn available_contexts(
    source: KubeconfigSource<Option<String>>,
) -> PluginResult<Vec<String>> {
    let kubeconfig = tokio::task::spawn_blocking(move || load_kubeconfig(&source))
        .await
        .map_err(|error| {
            PluginError::Internal(format!("Kubeconfig loader task failed: {error}"))
        })??;
    let contexts: BTreeSet<String> = kubeconfig
        .contexts
        .into_iter()
        .map(|context| context.name)
        .collect();
    if contexts.is_empty() {
        return Err(PluginError::InvalidConfig(
            "No valid kubeconfig files found or no contexts available".to_string(),
        ));
    }

    Ok(contexts.into_iter().collect())
}

#[cfg(test)]
mod tests;
