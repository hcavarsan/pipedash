use kube::config::Kubeconfig;
use pipedash_plugin_api::kubernetes::{
    read_kubeconfigs,
    KubeconfigSource,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

use crate::config::describe;

pub(super) fn load_kubeconfig(source: &KubeconfigSource<Vec<String>>) -> PluginResult<Kubeconfig> {
    match source {
        KubeconfigSource::Content(content) => Kubeconfig::from_yaml(content).map_err(|_| {
            PluginError::InvalidConfig(
                "The pasted kubeconfig content is not valid kubeconfig YAML".to_string(),
            )
        }),
        KubeconfigSource::Path(paths) => read_kubeconfigs(paths),
    }
}

pub(crate) async fn available_contexts(
    source: KubeconfigSource<Vec<String>>,
) -> PluginResult<Vec<String>> {
    tokio::task::spawn_blocking(move || {
        let mut contexts: Vec<String> = load_kubeconfig(&source)?
            .contexts
            .into_iter()
            .map(|context| context.name)
            .collect();
        if contexts.is_empty() {
            return Err(PluginError::InvalidConfig(format!(
                "No contexts found in kubeconfig: {}",
                describe(&source)
            )));
        }
        contexts.sort();
        contexts.dedup();

        Ok(contexts)
    })
    .await
    .map_err(|error| PluginError::Internal(format!("Kubeconfig loader task failed: {error}")))?
}

#[cfg(test)]
mod tests;
