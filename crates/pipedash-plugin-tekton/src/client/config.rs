use std::path::PathBuf;

use kube::config::Kubeconfig;
use pipedash_plugin_api::kubernetes::{
    self,
    KubeconfigSource,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

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

pub(super) fn load_kubeconfig(source: &KubeconfigSource) -> PluginResult<Kubeconfig> {
    match source {
        KubeconfigSource::Content(content) => kubernetes::parse_kubeconfig_content(content),
        KubeconfigSource::Path(paths) => merge_kubeconfigs(paths.clone()),
    }
}

pub(crate) async fn available_contexts(source: KubeconfigSource) -> PluginResult<Vec<String>> {
    let description = source.describe();
    kubernetes::available_contexts(move || load_kubeconfig(&source), description).await
}

#[cfg(test)]
mod tests;
