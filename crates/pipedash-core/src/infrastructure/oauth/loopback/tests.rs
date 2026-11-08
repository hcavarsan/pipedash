use super::*;

pub(crate) async fn free_redirect_uri() -> Url {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    Url::parse(&format!("http://localhost:{port}/auth/callback")).unwrap()
}

pub(crate) async fn get(url: &Url) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", url.port().unwrap()))
        .await
        .unwrap();
    let target = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    };
    stream
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    (status, response)
}

#[tokio::test]
async fn accept_skips_other_paths_and_returns_the_callback() {
    let redirect_uri = free_redirect_uri().await;
    let loopback = Loopback::bind(&redirect_uri).await.unwrap();
    let idle = TcpStream::connect(("127.0.0.1", redirect_uri.port().unwrap()))
        .await
        .unwrap();

    let favicon = redirect_uri.join("/favicon.ico").unwrap();
    let callback = redirect_uri.join("/auth/callback?code=c&state=s").unwrap();
    let client = tokio::spawn(async move { (get(&favicon).await.0, get(&callback).await) });

    let (stream, url) = loopback.accept().await.unwrap();
    respond(stream, &Err("<bad>".into())).await;
    let (favicon_status, (callback_status, body)) = client.await.unwrap();

    assert_eq!(url.query(), Some("code=c&state=s"));
    assert_eq!(favicon_status, 404);
    assert_eq!(callback_status, 400);
    assert!(body.contains("&lt;bad&gt;"));
    drop(idle);
}

#[tokio::test]
async fn bind_rejects_non_loopback_redirects() {
    for uri in [
        "https://localhost:8085/cb",
        "http://example.com:8085/cb",
        "http://localhost/cb",
    ] {
        assert!(Loopback::bind(&Url::parse(uri).unwrap()).await.is_none());
    }
}
