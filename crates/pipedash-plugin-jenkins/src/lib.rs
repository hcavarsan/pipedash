//! Jenkins plugin for Pipedash
//!
//! This plugin provides integration with Jenkins CI/CD platform, allowing you
//! to:
//! - Monitor jobs and builds
//! - View build history and details
//! - Trigger builds with parameters
//!
//! # Architecture
//!
//! The plugin is organized into several modules:
//! - `plugin` - Main plugin implementation
//! - `client` - Jenkins API client methods
//! - `types` - API response types
//! - `mapper` - Data mapping utilities
//! - `config` - Configuration parsing
//!
//! # Example Usage
//!
//! ```no_run
//! use pipedash_plugin_jenkins::JenkinsPlugin;
//! use pipedash_plugin_api::{Plugin, PluginRegistry};
//!
//! let mut registry = PluginRegistry::new();
//! registry.register(Box::new(JenkinsPlugin::new()));
//! ```

mod client;
mod config;
mod mapper;
mod plugin;
mod types;

// Re-export the plugin struct
pub use plugin::JenkinsPlugin;

// Register plugin with the registry
pipedash_plugin_api::register_plugin!(JenkinsPlugin);
