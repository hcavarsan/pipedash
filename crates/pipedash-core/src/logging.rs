use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

pub const DEFAULT_LOG_FILTER: &str =
    "pipedash=info,pipedash_core=info,pipedash_api=info,tower_http=info";

pub fn init() {
    init_with_default(DEFAULT_LOG_FILTER);
}

pub fn init_with_default(default_filter: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .init();
}

pub fn init_dev() {
    init_with_default("pipedash=debug,pipedash_core=debug,pipedash_api=debug,tower_http=debug");
}
