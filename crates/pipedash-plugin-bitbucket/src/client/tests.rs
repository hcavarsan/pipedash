use std::io::{
    Read,
    Write,
};
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::Arc;

use super::*;

fn page(from: i64, count: i64, next: Option<String>) -> String {
    let values: Vec<serde_json::Value> = (from..from + count)
        .rev()
        .map(|n| {
            serde_json::json!({
                "uuid": format!("{{{n}}}"),
                "build_number": n,
                "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}},
                "target": {"type": "pipeline_ref_target", "ref_name": "main"},
                "created_on": "2026-10-01T00:00:00Z",
                "links": {}
            })
        })
        .collect();
    serde_json::json!({"values": values, "next": next, "pagelen": 100}).to_string()
}

fn serve_two_pages() -> (BitbucketClient, Arc<AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let line = request.lines().next().unwrap_or_default().to_string();
            counter.fetch_add(1, Ordering::SeqCst);
            let body = if line.contains("page=2") {
                page(1, 50, None)
            } else {
                page(
                    51,
                    100,
                    Some(format!(
                        "http://127.0.0.1:{port}/repositories/ws/repo/pipelines?pagelen=100&sort=-created_on&page=2"
                    )),
                )
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });

    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = BitbucketClient::new(
        Arc::new(reqwest::Client::new()),
        format!("http://127.0.0.1:{port}"),
        "Basic x".to_string(),
    );
    (client, requests)
}

#[tokio::test]
async fn run_history_follows_next_past_the_first_100_runs() {
    let (client, requests) = serve_two_pages();

    let runs = client.list_pipelines("ws", "repo", 200).await.unwrap();

    assert_eq!(runs.len(), 150);
    assert_eq!(runs.first().map(|run| run.build_number), Some(150));
    assert_eq!(runs.last().map(|run| run.build_number), Some(1));
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn run_history_stops_at_the_limit() {
    let (client, requests) = serve_two_pages();

    let runs = client.list_pipelines("ws", "repo", 120).await.unwrap();
    assert_eq!(runs.len(), 120);
    assert_eq!(runs.last().map(|run| run.build_number), Some(31));
    assert_eq!(requests.load(Ordering::SeqCst), 2);

    let runs = client.list_pipelines("ws", "repo", 100).await.unwrap();
    assert_eq!(runs.len(), 100);
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn run_history_follows_the_eleventh_page_to_a_partial_final_page() {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let count = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            let target = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let url = reqwest::Url::parse(&format!("http://{address}{target}")).unwrap();
            let page_number = url
                .query_pairs()
                .find(|(key, _)| key == "page")
                .map(|(_, value)| value.parse::<i64>().unwrap())
                .unwrap_or(1);
            let offset = (page_number - 1) * 100;
            let count = (1050 - offset).clamp(0, 100);
            let next = (offset + count < 1050).then(|| {
                format!(
                    "http://{address}/repositories/ws/repo/pipelines?pagelen=100&page={}",
                    page_number + 1
                )
            });
            let body = page(1050 - offset - count + 1, count, next);
            counter.fetch_add(1, Ordering::SeqCst);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = BitbucketClient::new(
        Arc::new(reqwest::Client::new()),
        format!("http://{address}"),
        "Basic fixture".to_string(),
    );

    let runs = client.list_pipelines("ws", "repo", 1100).await.unwrap();
    server.abort();
    let numbers: Vec<_> = runs.iter().map(|run| run.build_number).collect();
    assert_eq!(numbers, (1..=1050).rev().collect::<Vec<_>>());
    assert_eq!(requests.load(Ordering::SeqCst), 11);
}
