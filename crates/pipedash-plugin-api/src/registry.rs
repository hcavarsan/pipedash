use std::collections::HashMap;

use crate::plugin::Plugin;

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let provider_type = plugin.provider_type().to_string();
        self.plugins.insert(provider_type, plugin);
    }

    pub fn get(&self, provider_type: &str) -> Option<&dyn Plugin> {
        self.plugins.get(provider_type).map(|p| p.as_ref())
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
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.count(), 0);
    }
}
