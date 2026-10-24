mod apns;
mod config;
mod rate_limit;
mod routes;
mod store;
#[cfg(test)]
mod test_support;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::apns::{
    unix_now,
    Apns,
    Signer,
};
use crate::config::Config;
use crate::routes::AppState;
use crate::store::Store;

const INACTIVITY_LIMIT: Duration = Duration::from_secs(90 * 24 * 60 * 60);
const EXPIRY_SWEEP: Duration = Duration::from_secs(60 * 60);
const LIMITER_SWEEP: Duration = Duration::from_secs(5 * 60);

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

fn spawn_expiry(store: Store) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(EXPIRY_SWEEP);
        loop {
            tick.tick().await;
            let cutoff = unix_now().saturating_sub(INACTIVITY_LIMIT.as_secs());
            match store.delete_inactive_since(cutoff).await {
                Ok(0) => {}
                Ok(expired) => tracing::info!(expired, "Removed inactive subscriptions"),
                Err(error) => tracing::warn!(%error, "Couldn't remove inactive subscriptions"),
            }
        }
    });
}

fn spawn_limiter_sweep(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(LIMITER_SWEEP);
        loop {
            tick.tick().await;
            state.prune_limiters(Instant::now());
        }
    });
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
    spawn_expiry(store.clone());
    let state = AppState::new(
        store,
        Arc::new(apns),
        config.allowed_bundle_ids,
        config.trusted_proxy,
    );
    spawn_limiter_sweep(state.clone());
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("Couldn't listen on {}", config.bind_addr))?;
    tracing::info!("Push relay listening on {}", config.bind_addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}
