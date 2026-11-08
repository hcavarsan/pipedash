use std::sync::{
    Arc,
    OnceLock,
};

use pipedash_plugin_api::PluginRegistry;

static REGISTRY: OnceLock<Arc<PluginRegistry>> = OnceLock::new();

pub(crate) fn plugin_registry() -> Arc<PluginRegistry> {
    Arc::clone(REGISTRY.get_or_init(|| Arc::new(pipedash_plugins::registry())))
}
