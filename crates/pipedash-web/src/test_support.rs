use std::path::PathBuf;
use std::sync::{
    Arc,
    Mutex,
};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{
    Json,
    Router,
};
use pipedash_core::infrastructure::{
    PipedashConfig,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::CoreContext;
use serde_json::Value;

use crate::routes::CONFIG_ENV;
use crate::state::AppState;
use crate::ws::WebSocketEventBus;

#[derive(Clone)]
struct RelayState {
    requests: Arc<Mutex<Vec<Value>>>,
    status: Arc<Mutex<u16>>,
}

pub(crate) struct StubRelay {
    pub url: String,
    state: RelayState,
}

async fn accept_notification(
    State(relay): State<RelayState>, Json(body): Json<Value>,
) -> StatusCode {
    relay.requests.lock().unwrap().push(body);
    let status = *relay.status.lock().unwrap();

    StatusCode::from_u16(status).unwrap()
}

pub(crate) async fn stub_server() -> StubRelay {
    let state = RelayState {
        requests: Arc::default(),
        status: Arc::new(Mutex::new(202)),
    };
    let app = Router::new()
        .route("/v1/notifications", post(accept_notification))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    StubRelay { url, state }
}

impl StubRelay {
    pub(crate) fn reply_with(&self, status: u16) {
        *self.state.status.lock().unwrap() = status;
    }

    pub(crate) fn requests(&self) -> Vec<Value> {
        self.state.requests.lock().unwrap().clone()
    }
}

pub(crate) struct Hosted {
    pub state: AppState,
    data_dir: PathBuf,
    _storage: Option<StorageManager>,
}

impl Drop for Hosted {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

pub(crate) async fn hosted(name: &str) -> Hosted {
    build(name, true).await
}

pub(crate) async fn server(name: &str) -> Hosted {
    build(name, false).await
}

async fn build(name: &str, shared: bool) -> Hosted {
    let _env = CONFIG_ENV.lock().await;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-hosted-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(
        data_dir.join("config.toml"),
        format!(
            "[storage]\ndata_dir = {:?}\n",
            data_dir.display().to_string()
        ),
    )
    .unwrap();
    std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();
    config.storage.vault_password = Some("vault-password".into());
    let storage = StorageManager::from_config(config, false).await.unwrap();

    let bus = Arc::new(WebSocketEventBus::new());
    let core = CoreContext::with_storage_manager(&storage, bus.clone())
        .await
        .unwrap();

    if shared {
        Hosted {
            state: AppState::hosted(bus, Some(core), "test-mac"),
            data_dir,
            _storage: Some(storage),
        }
    } else {
        Hosted {
            state: AppState::initialized(bus, core, storage),
            data_dir,
            _storage: None,
        }
    }
}
