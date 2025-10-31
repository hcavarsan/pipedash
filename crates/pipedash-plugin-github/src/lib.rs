//! GitHub Actions plugin for Pipedash
//!
//! This plugin provides integration with GitHub Actions, allowing you to:
//! - Monitor workflows and runs
//! - View run history and details
//! - Trigger workflow dispatches
//!
//! # Architecture
//!
//! The plugin is organized into several modules:
//! - `plugin` - Main plugin implementation
//! - `client` - GitHub API client methods
//! - `types` - Type re-exports from octocrab
//! - `mapper` - Data mapping utilities
//! - `config` - Configuration parsing
//!
//! # Example Usage
//!
//! ```no_run
//! use pipedash_plugin_github::GitHubPlugin;
//! use pipedash_plugin_api::{Plugin, PluginRegistry};
//!
//! let mut registry = PluginRegistry::new();
//! registry.register(Box::new(GitHubPlugin::new()));
//! ```

mod client;
mod config;
mod mapper;
mod plugin;
mod types;

// Re-export the plugin struct
pub use plugin::GitHubPlugin;

// Register plugin with the registry
pipedash_plugin_api::register_plugin!(GitHubPlugin);
