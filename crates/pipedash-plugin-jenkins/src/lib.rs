mod client;
mod config;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
#[cfg(test)]
mod test_support;
mod types;

pub use log_dialect::JenkinsLogDialect;
pub use plugin::JenkinsPlugin;

pipedash_plugin_api::register_plugin!(JenkinsPlugin);
