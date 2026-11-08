use std::sync::Arc;

use pipedash_core::domain::ServerUrl;
use pipedash_core::infrastructure::{
    PipedashConfig,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::CoreContext;
use pipedash_web::state::AppState;
use pipedash_web::ws::event_bus::WebSocketEventBus;
use tokio::sync::broadcast;

use super::session::protocol::ClientHello;

pub struct Host {
    pub url: ServerUrl,
    pub state: AppState,
    pub core: CoreContext,
    _storage: StorageManager,
    _dir: tempfile::TempDir,
}

pub async fn start_host() -> Host {
    start_host_with_relay(None).await
}

pub async fn start_host_with_relay(relay: Option<&str>) -> Host {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        format!(
            "[storage]\ndata_dir = {:?}\n",
            dir.path().display().to_string()
        ),
    )
    .unwrap();
    std::env::set_var("PIPEDASH_CONFIG_PATH", dir.path().join("config.toml"));
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = dir.path().display().to_string();
    config.storage.vault_password = Some("vault-password".into());
    let storage = StorageManager::from_config(config, false).await.unwrap();

    let bus = Arc::new(WebSocketEventBus::from_sender(broadcast::channel(64).0));
    let core = CoreContext::with_storage_manager(
        &storage,
        bus.clone(),
        crate::plugin_registry::plugin_registry(),
    )
    .await
    .unwrap();
    let mut state = AppState::hosted(
        bus,
        Some(core.clone()),
        "Studio",
        crate::plugin_registry::plugin_registry(),
    );
    if let Some(relay) = relay {
        state = state.with_push_relay(relay);
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = ServerUrl::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    tokio::spawn(pipedash_web::serve_host(
        listener,
        state.clone(),
        std::future::pending(),
    ));

    Host {
        url,
        state,
        core,
        _storage: storage,
        _dir: dir,
    }
}

pub fn claim(code: &str) -> ClientHello<'_> {
    ClientHello::Claim {
        code,
        name: "Test phone",
        platform: "ios",
    }
}
