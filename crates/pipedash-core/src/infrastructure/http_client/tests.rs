use super::*;

fn options(origin: &str, accept_invalid_certs: bool) -> ClientOptions {
    ClientOptions {
        origin: Some(Url::parse(origin).unwrap()),
        accept_invalid_certs,
        ..ClientOptions::default()
    }
}

static HTTPS_CERTIFICATE: std::sync::LazyLock<rcgen::CertifiedKey<rcgen::KeyPair>> =
    std::sync::LazyLock::new(|| {
        let now = std::time::SystemTime::now();
        let mut params =
            rcgen::CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
        params.not_before = (now - Duration::from_secs(60)).into();
        params.not_after = (now + Duration::from_secs(86400)).into();
        params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let signing_key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&signing_key).unwrap();
        rcgen::CertifiedKey { cert, signing_key }
    });

fn https_only_client() -> Client {
    HttpClientManager::base_builder(None)
        .unwrap()
        .https_only(true)
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(HTTPS_CERTIFICATE.cert.der()).unwrap())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

async fn tls_stub(response: String) -> (String, tokio::task::JoinHandle<String>) {
    use std::io::{
        Read,
        Write,
    };

    let _ = rustls::crypto::ring::default_provider().install_default();
    let certificate = &*HTTPS_CERTIFICATE;
    let cert = certificate.cert.der().clone();
    let key =
        rustls::pki_types::PrivateKeyDer::Pkcs8(certificate.signing_key.serialize_der().into());
    let config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "https://127.0.0.1:{}",
        listener.local_addr().unwrap().port()
    );
    let server = tokio::spawn(async move {
        let (stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let stream = stream.into_std().unwrap();
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        tokio::task::spawn_blocking(move || {
            let connection = rustls::ServerConnection::new(config).unwrap();
            let mut stream = rustls::StreamOwned::new(connection, stream);
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0 && request.len() + count <= 8192);
                request.extend_from_slice(&buffer[..count]);
            }
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
            String::from_utf8(request).unwrap()
        })
        .await
        .unwrap()
    });
    (url, server)
}

#[tokio::test]
async fn https_only_rejects_http_before_sending_credentials() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let error = https_only_client()
        .get(format!("http://{}", listener.local_addr().unwrap()))
        .header("Authorization", "token fixture")
        .send()
        .await
        .unwrap_err();

    assert!(error.is_builder(), "{error}");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn https_only_allows_https_with_credentials() {
    let (url, server) =
        tls_stub("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_string())
            .await;
    let response = https_only_client()
        .get(url)
        .header("Authorization", "token fixture")
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "ok");
    assert!(server
        .await
        .unwrap()
        .to_ascii_lowercase()
        .contains("authorization: token fixture"));
}

#[tokio::test]
async fn https_only_rejects_http_downgrade_redirect_before_traffic() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (url, server) = tls_stub(format!(
        "HTTP/1.1 302 Found\r\nLocation: http://{}/token\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        listener.local_addr().unwrap()
    ))
    .await;
    let error = https_only_client()
        .get(url)
        .header("Authorization", "token fixture")
        .send()
        .await
        .unwrap_err();

    assert!(error.is_redirect(), "{error}");
    assert!(server
        .await
        .unwrap()
        .to_ascii_lowercase()
        .contains("authorization: token fixture"));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

async fn redirect_stub(location: String) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/start", listener.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let location = location.clone();
            tokio::spawn(async move {
                use tokio::io::{
                    AsyncReadExt,
                    AsyncWriteExt,
                };
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..count]);
                let response = if request.starts_with("GET /start ") {
                    format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                        .to_string()
                };
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    url
}

#[tokio::test]
async fn same_host_redirect_clients_refuse_redirects_to_another_host() {
    let manager = HttpClientManager::new().unwrap();
    let guarded = ClientOptions {
        same_host_redirects: true,
        ..ClientOptions::default()
    };
    let client = manager.client_for(&guarded).unwrap();
    assert!(!Arc::ptr_eq(
        &client,
        &manager.client_for(&ClientOptions::default()).unwrap()
    ));

    let same = redirect_stub("/done".to_string()).await;
    let response = client.get(&same).send().await.unwrap();
    assert_eq!(response.text().await.unwrap(), "ok");

    let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let elsewhere = redirect_stub(format!(
        "http://localhost:{}/secret",
        other.local_addr().unwrap().port()
    ))
    .await;
    let error = client.get(&elsewhere).send().await.unwrap_err();
    assert!(error.is_redirect(), "{error}");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), other.accept())
            .await
            .is_err()
    );
}

#[test]
fn redirects_stay_on_the_host_and_never_downgrade() {
    let url = |value: &str| Url::parse(value).unwrap();
    assert!(stays_on_host(
        &url("https://a.example/x"),
        &url("https://a.example:8443/y")
    ));
    assert!(stays_on_host(
        &url("http://a.example/x"),
        &url("https://a.example/y")
    ));
    assert!(!stays_on_host(
        &url("https://a.example/x"),
        &url("http://a.example/y")
    ));
    assert!(!stays_on_host(
        &url("https://a.example/x"),
        &url("https://169.254.169.254/")
    ));
}

#[cfg(target_os = "ios")]
#[tokio::test]
async fn ios_shared_clients_reject_http_in_every_cache_and_proxy_configuration() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let manager = HttpClientManager::new().unwrap();
    for proxy in ["", url.as_str()] {
        manager.set_proxy(proxy).unwrap();
        let clients = [
            manager.client_for(&ClientOptions::default()).unwrap(),
            manager.client_for(&options(&url, false)).unwrap(),
            manager.client_for(&options(&url, true)).unwrap(),
            manager.status_client().unwrap(),
        ];
        for client in clients {
            let error = client
                .get(&url)
                .header("Authorization", "token fixture")
                .send()
                .await
                .unwrap_err();
            assert!(error.is_builder(), "{error}");
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn default_options_share_the_default_client() {
    let manager = HttpClientManager::new().unwrap();
    let a = manager.client_for(&ClientOptions::default()).unwrap();
    let b = manager.client_for(&ClientOptions::default()).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn equivalent_origins_share_one_client() {
    let manager = HttpClientManager::new().unwrap();
    let a = manager.client_for(&options("https://h", false)).unwrap();
    let b = manager
        .client_for(&options("https://H:443/", false))
        .unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(!Arc::ptr_eq(&a, &manager.current().default_client));
}

#[test]
fn insecure_and_secure_clients_differ_for_same_origin() {
    let manager = HttpClientManager::new().unwrap();
    let secure = manager
        .client_for(&options("https://status.example.test", false))
        .unwrap();
    let insecure = manager
        .client_for(&options("https://status.example.test", true))
        .unwrap();
    assert!(!Arc::ptr_eq(&secure, &insecure));
}

#[test]
fn different_origins_get_different_clients() {
    let manager = HttpClientManager::new().unwrap();
    let a = manager
        .client_for(&options("https://a.example", false))
        .unwrap();
    let b = manager
        .client_for(&options("https://a.example:8443", false))
        .unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
}

#[test]
fn status_client_is_cached_and_separate() {
    let manager = HttpClientManager::new().unwrap();
    let a = manager.status_client().unwrap();
    let b = manager.status_client().unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    let insecure = manager
        .client_for(&options("https://a.example", true))
        .unwrap();
    assert!(!Arc::ptr_eq(&a, &insecure));
    assert!(!Arc::ptr_eq(&a, &manager.current().default_client));
}

#[test]
fn redirect_targets_must_be_public_https() {
    for allowed in [
        "https://status.example.test/api/v2/summary.json",
        "https://8.8.8.8/",
    ] {
        assert!(
            is_safe_redirect_target(&Url::parse(allowed).unwrap()),
            "{allowed}"
        );
    }
    for refused in [
        "http://status.example.test/",
        "http://127.0.0.1:8765/",
        "https://127.0.0.1/",
        "https://10.1.2.3/",
        "https://192.168.0.1/",
        "https://169.254.169.254/",
        "https://0.0.0.0/",
        "https://[::1]/",
        "https://[fe80::1]/",
        "https://[fd00::1]/",
        "https://[::ffff:127.0.0.1]/",
    ] {
        assert!(
            !is_safe_redirect_target(&Url::parse(refused).unwrap()),
            "{refused}"
        );
    }
}

#[cfg(not(target_os = "ios"))]
async fn proxy_stub() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0; 4096];
            let n = stream.read(&mut buf).await.unwrap();
            let head = String::from_utf8_lossy(&buf[..n]);
            let line = head.lines().next().unwrap_or_default().to_string();
            log.lock().unwrap().push(line);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
        }
    });
    (url, seen)
}

#[cfg(not(target_os = "ios"))]
#[tokio::test]
async fn every_client_goes_through_the_proxy_until_it_is_cleared() {
    let (proxy, seen) = proxy_stub().await;
    let manager = HttpClientManager::new().unwrap();

    assert!(manager.set_proxy(&format!(" {proxy} ")).unwrap());
    assert!(!manager.set_proxy(&proxy).unwrap());
    let clients = [
        manager.client_for(&ClientOptions::default()).unwrap(),
        manager
            .client_for(&options("http://status.example.test", true))
            .unwrap(),
        manager.status_client().unwrap(),
    ];
    for (i, client) in clients.iter().enumerate() {
        let body = client
            .get(format!("http://host{i}.pipedash.test/api"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "ok");
    }

    assert!(manager.set_proxy("").unwrap());
    let direct = manager
        .client_for(&ClientOptions::default())
        .unwrap()
        .get("http://127.0.0.1:9/never-proxied")
        .send()
        .await;

    assert!(direct.is_err());
    assert_eq!(
        *seen.lock().unwrap(),
        [
            "GET http://host0.pipedash.test/api HTTP/1.1",
            "GET http://host1.pipedash.test/api HTTP/1.1",
            "GET http://host2.pipedash.test/api HTTP/1.1",
        ]
    );
}

#[test]
fn proxy_url_accepts_http_and_https_without_credentials() {
    assert_eq!(parse_proxy_url("  ").unwrap(), None);
    assert_eq!(
        parse_proxy_url(" http://proxy.corp:3128 ")
            .unwrap()
            .unwrap()
            .as_str(),
        "http://proxy.corp:3128/"
    );
    assert!(parse_proxy_url("https://proxy.corp").unwrap().is_some());

    for refused in [
        "proxy.corp:3128",
        "socks5://proxy.corp:1080",
        "http://",
        "not a url",
        "http://alice@proxy.corp:3128",
        "http://alice:secret@proxy.corp:3128",
    ] {
        let error = parse_proxy_url(refused).unwrap_err().to_string();
        assert!(!error.contains("secret"), "{error}");
        assert!(matches!(
            parse_proxy_url(refused),
            Err(DomainError::InvalidConfig(_))
        ));
    }
}
