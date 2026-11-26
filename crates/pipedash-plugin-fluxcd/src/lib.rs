mod actions;
mod client;
mod config;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod health;
mod history;
mod logs;
mod mapper;
mod metadata;
mod plugin;
mod pods;
mod presentation;
mod schema;
mod types;
mod workloads;

pub use plugin::FluxcdPlugin;

#[cfg(any(test, feature = "fixtures"))]
pub fn correlation_fixtures() -> Vec<(
    pipedash_plugin_api::Pipeline,
    Vec<pipedash_plugin_api::PipelineRun>,
)> {
    fixtures::all()
}

pipedash_plugin_api::register_plugin!(FluxcdPlugin);
