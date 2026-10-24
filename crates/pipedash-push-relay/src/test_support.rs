use std::sync::{
    Arc,
    Mutex,
};

use axum::extract::{
    Request,
    State,
};
use axum::http::{
    StatusCode,
    Version,
};
use axum::routing::post;
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{
    EcdsaKeyPair,
    KeyPair,
    ECDSA_P256_SHA256_FIXED_SIGNING,
};
use tokio::sync::Notify;

pub(crate) fn key_pem() -> (String, Vec<u8>) {
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
    let key_pair =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng).unwrap();
    let encoded = STANDARD.encode(pkcs8.as_ref());
    let lines: Vec<&str> = encoded
        .as_bytes()
        .chunks(64)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        lines.join("\n")
    );

    (pem, key_pair.public_key().as_ref().to_vec())
}

pub(crate) struct Recorded {
    pub path: String,
    pub version: Version,
    pub authorization: Option<String>,
    pub topic: Option<String>,
    pub push_type: Option<String>,
    pub body: serde_json::Value,
}

type Gate = (Arc<Notify>, Arc<Notify>);

#[derive(Clone)]
struct Shared {
    requests: Arc<Mutex<Vec<Recorded>>>,
    reply: Arc<Mutex<(u16, String)>>,
    gate: Arc<Mutex<Option<Gate>>>,
}

pub(crate) struct StubApns {
    pub url: String,
    shared: Shared,
}

async fn record(State(shared): State<Shared>, request: Request) -> (StatusCode, String) {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, 64 * 1024).await.unwrap();
    let header = |name: &str| {
        parts
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };

    shared.requests.lock().unwrap().push(Recorded {
        path: parts.uri.path().to_owned(),
        version: parts.version,
        authorization: header("authorization"),
        topic: header("apns-topic"),
        push_type: header("apns-push-type"),
        body: serde_json::from_slice(&body).unwrap(),
    });
    let gate = shared.gate.lock().unwrap().clone();
    if let Some((arrived, release)) = gate {
        arrived.notify_one();
        release.notified().await;
    }
    let (status, body) = shared.reply.lock().unwrap().clone();

    (StatusCode::from_u16(status).unwrap(), body)
}

impl StubApns {
    pub(crate) async fn start() -> Self {
        let shared = Shared {
            requests: Arc::new(Mutex::new(Vec::new())),
            reply: Arc::new(Mutex::new((200, String::new()))),
            gate: Arc::default(),
        };
        let app = Router::new()
            .route("/3/device/{token}", post(record))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        Self { url, shared }
    }

    pub(crate) fn reply_with(&self, status: u16, body: &str) {
        *self.shared.reply.lock().unwrap() = (status, body.to_string());
    }

    pub(crate) fn hold_replies(&self) -> (Arc<Notify>, Arc<Notify>) {
        let gate = (Arc::new(Notify::new()), Arc::new(Notify::new()));
        *self.shared.gate.lock().unwrap() = Some(gate.clone());

        gate
    }

    pub(crate) fn take_requests(&self) -> Vec<Recorded> {
        std::mem::take(&mut *self.shared.requests.lock().unwrap())
    }
}
