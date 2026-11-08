use std::io::{
    Read,
    Write,
};
use std::net::TcpListener;
use std::sync::mpsc;

use super::*;

fn serve_once(response: &'static str) -> (Url, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/log", listener.local_addr().unwrap())).unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let read = stream.read(&mut buf).unwrap_or(0);
            let _ = tx.send(String::from_utf8_lossy(&buf[..read]).to_lowercase());
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, rx)
}

async fn fetch(url: &Url, from: Option<u64>) -> PluginResult<Response> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::new();
    send_log(&RetryPolicy::default(), url, from, "gone", || {
        client.get(url.clone())
    })
    .await
}

#[tokio::test]
async fn resumed_requests_ask_for_the_remaining_bytes() {
    let (url, request) = serve_once("HTTP/1.1 206 Partial Content\r\nContent-Length: 0\r\n\r\n");
    assert_eq!(fetch(&url, Some(42)).await.unwrap().status(), 206);
    assert!(request.recv().unwrap().contains("range: bytes=42-"));
}

#[tokio::test]
async fn missing_or_expired_logs_are_not_found() {
    for status in ["404 Not Found", "410 Gone"] {
        let raw: &'static str =
            Box::leak(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").into_boxed_str());
        let (url, request) = serve_once(raw);
        let error = fetch(&url, None).await.unwrap_err();
        assert!(matches!(error, PluginError::PipelineNotFound(ref m) if m == "gone"));
        assert!(!request.recv().unwrap().contains("range:"));
    }
}
