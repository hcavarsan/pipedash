mod apns;
mod config;
mod rate_limit;
mod routes;
mod store;
#[cfg(test)]
mod test_support;

use std::sync::Arc;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::apns::{
    Apns,
    Signer,
};
use crate::config::Config;
use crate::routes::AppState;
use crate::store::Store;

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env()?;
    let signer = Signer::new(&config.apns_key, &config.apns_key_id, &config.apns_team_id)
        .context("Invalid APNs key")?;
    let apns = Apns::new(signer, apns::PRODUCTION_URL, apns::SANDBOX_URL)
        .context("Couldn't build the APNs client")?;
    let store = Store::open(&config.database_path)
        .await
        .context("Couldn't open the database")?;
    let app = routes::router(AppState::new(
        store,
        Arc::new(apns),
        config.allowed_bundle_ids,
    ));

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("Couldn't listen on {}", config.bind_addr))?;
    tracing::info!("Push relay listening on {}", config.bind_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
