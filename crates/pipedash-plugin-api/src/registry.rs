use std::collections::HashMap;

use crate::metadata::{
    PluginManifest,
    PluginMetadata,
};
use crate::plugin::Plugin;

struct Registration {
    plugin: Box<dyn Plugin>,
    metadata: PluginMetadata,
    create: fn() -> Box<dyn Plugin>,
}

pub struct PluginRegistry {
    plugins: HashMap<String, Registration>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, create: fn() -> Box<dyn Plugin>, manifest: PluginManifest) {
        let plugin = create();
        let mut metadata = plugin.metadata().clone();
        manifest.apply(&mut metadata);
        self.plugins.insert(
            manifest.provider_type().to_string(),
            Registration {
                plugin,
                metadata,
                create,
            },
        );
    }

    pub fn get(&self, provider_type: &str) -> Option<&dyn Plugin> {
        self.plugins
            .get(provider_type)
            .map(|registration| registration.plugin.as_ref())
    }

    pub fn create(&self, provider_type: &str) -> Option<Box<dyn Plugin>> {
        self.plugins
            .get(provider_type)
            .map(|registration| (registration.create)())
    }

    pub fn metadata(&self) -> impl Iterator<Item = &PluginMetadata> {
        self.plugins
            .values()
            .map(|registration| &registration.metadata)
    }

    pub fn metadata_of(&self, provider_type: &str) -> Option<&PluginMetadata> {
        self.plugins
            .get(provider_type)
            .map(|registration| &registration.metadata)
    }

    pub fn is_registered(&self, provider_type: &str) -> bool {
        self.plugins.contains_key(provider_type)
    }

    pub fn provider_types(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
