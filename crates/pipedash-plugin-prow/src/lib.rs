mod config;
mod deck;
mod mapper;
mod metadata;
mod plugin;
mod prowjobs;
mod schema;
mod sign_in;
mod storage;
#[cfg(test)]
mod test_support;
mod types;

pub use plugin::ProwPlugin;

pipedash_plugin_api::register_plugin!(ProwPlugin);
