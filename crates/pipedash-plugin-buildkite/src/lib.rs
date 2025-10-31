//! Buildkite plugin for Pipedash
//!
//! This plugin provides integration with Buildkite CI/CD platform, allowing you
//! to:
//! - Monitor pipelines and builds
//! - View build agents and their status
//! - Download build artifacts
//! - Trigger new builds
//!
//! # Architecture
//!
//! The plugin is organized into several modules:
//! - `plugin` - Main plugin implementation
//! - `client` - HTTP client and API methods
//! - `types` - API response types
//! - `mapper` - Data mapping utilities
//! - `config` - Configuration parsing
//!
//! # Example Usage
//!
//! ```no_run
//! use pipedash_plugin_buildkite::BuildkitePlugin;
//! use pipedash_plugin_api::{Plugin, PluginRegistry};
//!
//! let mut registry = PluginRegistry::new();
//! registry.register(Box::new(BuildkitePlugin::new()));
//! ```

mod client;
mod config;
mod mapper;
mod plugin;
mod types;

// Re-export the plugin struct
pub use plugin::BuildkitePlugin;

// Register plugin with the registry
pipedash_plugin_api::register_plugin!(BuildkitePlugin);
