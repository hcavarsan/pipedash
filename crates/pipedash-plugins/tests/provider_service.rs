#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::io::{
    Read,
    Write,
};
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_core::domain::{
    DomainError,
    FailureKind,
    FetchStatus,
    ProviderConfig,
    ProviderRecovery,
    RefreshState,
    SignInCredential,
};
use pipedash_core::infrastructure::MemoryTokenStore;
use pipedash_core::{
    CoreContext,
    NoOpEventBus,
};
use pipedash_plugin_api::PluginError;
use tempfile::TempDir;
use tokio::sync::Mutex;

static ENV: std::sync::OnceLock<Arc<Mutex<()>>> = std::sync::OnceLock::new();

struct Harness {
    core: CoreContext,
    temp_dir: TempDir,
    _env: tokio::sync::OwnedMutexGuard<()>,
}

impl Harness {
    async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let env = Arc::clone(ENV.get_or_init(Arc::default)).lock_owned().await;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(
            temp_dir.path().join("config.toml"),
            format!(
                "[storage]\ndata_dir = {:?}\n",
                temp_dir.path().display().to_string()
            ),
        )
        .unwrap();
        std::env::set_var("PIPEDASH_CONFIG_PATH", temp_dir.path().join("config.toml"));
        let core = CoreContext::new(
            temp_dir.path(),
            Arc::new(NoOpEventBus),
            Arc::new(MemoryTokenStore::new()),
            Arc::new(pipedash_plugins::registry()),
        )
        .await
        .unwrap();
        Self {
            core,
            temp_dir,
            _env: env,
        }
    }

    fn config(&self, name: &str, refresh_interval: i64) -> ProviderConfig {
        provider(name, "argocd", "token", refresh_interval, &[])
    }
}

fn provider(
    name: &str, provider_type: &str, token: &str, refresh_interval: i64, config: &[(&str, &str)],
) -> ProviderConfig {
    ProviderConfig {
        id: None,
        name: name.to_string(),
        provider_type: provider_type.to_string(),
        token: token.to_string(),
        config: config
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        refresh_interval,
        version: None,
    }
}

async fn stub_github(remaining: u64, reset: i64) -> (String, Arc<Mutex<Vec<String>>>) {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = requests.clone();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(request).unwrap();
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let path = path.split('?').next().unwrap().to_string();
            let (status, body) = if path == "/api/v3/rate_limit" {
                let rate = serde_json::json!({
                    "limit": 5000, "used": 5000 - remaining, "remaining": remaining, "reset": reset
                });
                let search =
                    serde_json::json!({"limit": 30, "used": 0, "remaining": 30, "reset": reset});
                let body = serde_json::json!({
                    "resources": {"core": rate, "search": search},
                    "rate": rate
                });
                ("200 OK", body.to_string())
            } else {
                ("404 Not Found", r#"{"message":"Not Found"}"#.to_string())
            };
            log.lock().await.push(path);
            let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    (format!("http://{addr}"), requests)
}

#[tokio::test]
async fn refresh_skips_a_github_provider_with_its_quota_almost_used_up() {
    let h = Harness::new().await;
    h.core.refresh_manager.start().await;
    let reset = Utc::now().timestamp() + 1200;
    let (base_url, requests) = stub_github(0, reset).await;
    let id = h
        .core
        .provider_service
        .repository()
        .add_provider(&provider(
            "github",
            "github",
            "ghp_stub",
            30,
            &[("base_url", &base_url), ("selected_items", "acme/app")],
        ))
        .await
        .unwrap();

    let first = h
        .core
        .pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let again = h
        .core
        .pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let summaries = h.core.provider_service.list_providers().await.unwrap();

    assert_eq!(*requests.lock().await, ["/api/v3/rate_limit"]);
    assert_eq!(
        summaries[0].paused_until,
        DateTime::from_timestamp(reset, 0)
    );
    assert!(matches!(summaries[0].last_fetch_status, FetchStatus::Never));
    assert_eq!(first[0].state, RefreshState::Recent);
    assert_eq!(again[0].state, RefreshState::Recent);
    assert_eq!(again[0].kind, Some(FailureKind::RateLimited));
    assert_eq!(*requests.lock().await, ["/api/v3/rate_limit"]);
}

#[tokio::test]
async fn refresh_fetches_a_github_provider_with_quota_left() {
    let h = Harness::new().await;
    h.core.refresh_manager.start().await;
    let (base_url, requests) = stub_github(4000, Utc::now().timestamp() + 1200).await;
    let id = h
        .core
        .provider_service
        .repository()
        .add_provider(&provider(
            "github",
            "github",
            "ghp_stub",
            30,
            &[("base_url", &base_url), ("selected_items", "acme/app")],
        ))
        .await
        .unwrap();

    h.core
        .pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let summaries = h.core.provider_service.list_providers().await.unwrap();
    let requests = requests.lock().await;

    assert_eq!(requests[0], "/api/v3/rate_limit");
    assert!(requests
        .iter()
        .any(|path| path.starts_with("/api/v3/repos/acme/app")));
    assert_eq!(summaries[0].paused_until, None);
}

#[tokio::test]
async fn field_options_work_when_the_plugin_rejects_the_partial_form() {
    let h = Harness::new().await;
    let kubeconfig = h.temp_dir.path().join("kubeconfig");
    std::fs::write(
            &kubeconfig,
            "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster: {server: 'https://127.0.0.1:6443'}\nusers:\n- name: u\n  user: {token: t}\ncontexts:\n- name: ctx-b\n  context: {cluster: c, user: u}\n- name: ctx-a\n  context: {cluster: c, user: u}\ncurrent-context: ctx-a\n",
        )
        .unwrap();
    let config = HashMap::from([
        ("kubeconfig".to_string(), kubeconfig.display().to_string()),
        ("namespaces".to_string(), "Not_A_Namespace".to_string()),
    ]);
    assert!(h
        .core
        .provider_service
        .init_plugin("fluxcd", config.clone(), 0)
        .await
        .is_err());

    let contexts = h
        .core
        .provider_service
        .field_options("fluxcd", "context", config)
        .await
        .unwrap();

    assert_eq!(contexts, ["ctx-a", "ctx-b"]);
}

#[tokio::test]
async fn failed_field_options_report_why_the_plugin_could_not_initialize() {
    let h = Harness::new().await;
    let config = HashMap::from([
        (
            "kubeconfig".to_string(),
            h.temp_dir.path().join("missing").display().to_string(),
        ),
        ("namespaces".to_string(), "Not_A_Namespace".to_string()),
    ]);

    let rejected = h
        .core
        .provider_service
        .field_options("fluxcd", "namespaces", config)
        .await;

    assert!(
        matches!(rejected, Err(DomainError::InvalidConfig(message)) if message.contains("initialize"))
    );
}

fn serve_tls(stream: std::net::TcpStream, config: Arc<rustls::ServerConfig>) {
    let Ok(connection) = rustls::ServerConnection::new(config) else {
        return;
    };
    let mut tls = rustls::StreamOwned::new(connection, stream);
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match tls.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(count) => request.extend_from_slice(&buffer[..count]),
        }
    }
    let body = r#"{"items":[]}"#;
    let _ = write!(
            tls,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    let _ = tls.flush();
    tls.conn.send_close_notify();
    let _ = tls.flush();
}

fn self_signed_argocd() -> String {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = rustls::pki_types::CertificateDer::from(
        include_bytes!("../../pipedash-plugin-api/tests/fixtures/self_signed_cert.der").to_vec(),
    );
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
        include_bytes!("../../pipedash-plugin-api/tests/fixtures/self_signed_key.der")
            .to_vec()
            .into(),
    );
    let config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let config = Arc::clone(&config);
            std::thread::spawn(move || serve_tls(stream, config));
        }
    });

    format!("https://{addr}")
}

fn argocd_settings(url: &str, insecure: bool) -> HashMap<String, String> {
    HashMap::from([
        ("server_url".to_string(), url.to_string()),
        ("token".to_string(), "token".to_string()),
        ("insecure".to_string(), insecure.to_string()),
    ])
}

#[tokio::test]
async fn argocd_insecure_tls_is_honored_by_add_validate_and_preview() {
    let h = Harness::new().await;
    let service = &h.core.provider_service;
    let url = self_signed_argocd();
    let page = Some(pipedash_plugin_api::PaginationParams {
        page: 1,
        page_size: 10,
    });
    let add = |insecure| {
        let mut config = h.config("argo", 30);
        config.config = argocd_settings(&url, insecure);
        config.config.remove("token");
        service.add_provider(config)
    };

    let id = add(true).await.unwrap();
    let insecure = service
        .init_plugin("argocd", argocd_settings(&url, true), 0)
        .await
        .unwrap();
    let secure = service
        .init_plugin("argocd", argocd_settings(&url, false), 0)
        .await
        .unwrap();
    let rejected = add(false).await.unwrap_err();

    assert!(id > 0);
    assert!(insecure.validate_credentials().await.unwrap());
    assert!(insecure
        .fetch_available_pipelines_filtered(None, None, page.clone())
        .await
        .unwrap()
        .items
        .is_empty());
    assert!(matches!(
        secure.validate_credentials().await,
        Err(PluginError::Tls(_))
    ));
    assert!(matches!(
        secure
            .fetch_available_pipelines_filtered(None, None, page)
            .await,
        Err(PluginError::Tls(_))
    ));
    assert!(matches!(rejected, DomainError::InvalidConfig(message) if message.contains("TLS")));
}

struct Idp {
    token_endpoint: String,
    reply: Arc<std::sync::Mutex<(u16, String)>>,
}

impl Idp {
    async fn start() -> Self {
        use tokio::io::{
            AsyncReadExt,
            AsyncWriteExt,
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let idp = Self {
            token_endpoint: format!("http://{}/token", listener.local_addr().unwrap()),
            reply: Arc::new(std::sync::Mutex::new((200, "{}".into()))),
        };
        let reply = Arc::clone(&idp.reply);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let reply = Arc::clone(&reply);
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut buffer = [0; 4096];
                    loop {
                        if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&data[..end]).to_string();
                            let length = head
                                .lines()
                                .find_map(|line| {
                                    let (name, value) = line.split_once(':')?;
                                    name.eq_ignore_ascii_case("content-length")
                                        .then(|| value.trim().parse::<usize>().ok())?
                                })
                                .unwrap_or(0);
                            if data.len() >= end + 4 + length {
                                break;
                            }
                        }
                        match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => data.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let (status, body) = reply.lock().unwrap().clone();
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        idp
    }

    fn reply(&self, status: u16, body: serde_json::Value) {
        *self.reply.lock().unwrap() = (status, body.to_string());
    }
}

fn sign_in(token_endpoint: String, expires_in: chrono::Duration) -> SignInCredential {
    SignInCredential {
        token: "bearer".into(),
        refresh_token: Some("r1".into()),
        expires_at: Some(Utc::now() + expires_in),
        token_endpoint,
        client_id: "cli".into(),
        kind: pipedash_plugin_api::SignInToken::AccessToken,
        account: Some("dev@example.com".into()),
    }
}

async fn stored_sign_in(h: &Harness, id: i64) -> SignInCredential {
    SignInCredential::decode(
        &h.core
            .provider_service
            .repository()
            .get_provider(id)
            .await
            .unwrap()
            .token,
    )
    .unwrap()
}

async fn add_expired_sso_provider(h: &Harness, idp: &Idp) -> (i64, SignInCredential) {
    let id = h
        .core
        .provider_service
        .repository()
        .add_provider(&h.config("sso", 30))
        .await
        .unwrap();
    let expired = sign_in(idp.token_endpoint.clone(), chrono::Duration::minutes(-1));
    h.core
        .provider_service
        .repository()
        .store_token(id, &expired.encode().unwrap())
        .await
        .unwrap();
    (id, expired)
}

#[tokio::test]
async fn stored_dex_refresh_rejections_ask_to_sign_in_again() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    let (id, expired) = add_expired_sso_provider(&h, &idp).await;

    for description in [
        "Refresh token is invalid or has already been claimed by another client.",
        "Refresh token expired.",
    ] {
        idp.reply(
            400,
            serde_json::json!({"error": "invalid_request", "error_description": description}),
        );

        let error = h.core.provider_service.acquire(id).await.err().unwrap();

        assert!(matches!(
            error,
            DomainError::ProviderAuthenticationFailed(_)
        ));
        assert_eq!(
            ProviderRecovery::for_failure(Some(error.failure_kind()), true),
            Some(ProviderRecovery::SignIn)
        );
        assert_eq!(stored_sign_in(&h, id).await, expired);
    }
}

#[tokio::test]
async fn unknown_dex_invalid_request_stays_an_api_error() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    idp.reply(
        400,
        serde_json::json!({"error": "invalid_request", "error_description": "Unknown request"}),
    );
    let (id, _) = add_expired_sso_provider(&h, &idp).await;

    let error = h.core.provider_service.acquire(id).await.err().unwrap();

    assert!(matches!(error, DomainError::ApiError(_)));
}

#[tokio::test]
async fn dex_invalid_request_server_errors_stay_transient() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    idp.reply(
        503,
        serde_json::json!({
            "error": "invalid_request",
            "error_description": "Refresh token expired.",
        }),
    );
    let (id, _) = add_expired_sso_provider(&h, &idp).await;

    let error = h.core.provider_service.acquire(id).await.err().unwrap();

    assert!(matches!(error, DomainError::ServiceUnavailable { .. }));
}
