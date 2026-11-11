use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::sync::Mutex;

use super::*;

fn history_run(number: usize) -> serde_json::Value {
    let url = "http://stub.invalid/acme/app";
    serde_json::json!({
        "id": number, "workflow_id": 42, "node_id": "n", "name": "CI",
        "head_branch": "main", "head_sha": "abc", "run_number": number, "event": "push",
        "status": "completed", "conclusion": "success",
        "created_at": "2026-10-20T10:00:00Z", "updated_at": "2026-10-20T10:01:00Z",
        "url": url, "html_url": url, "jobs_url": url, "logs_url": url,
        "check_suite_url": url, "artifacts_url": url, "cancel_url": url,
        "rerun_url": url, "workflow_url": url,
        "head_commit": {
            "id": "abc", "tree_id": "def", "message": "ship it",
            "timestamp": "2026-10-20T09:59:00Z",
            "author": {"name": "Ada", "email": "ada@example.com"},
            "committer": {"name": "Ada", "email": "ada@example.com"}
        },
        "repository": {"id": 1, "name": "app", "url": url}
    })
}

async fn history_fixture() -> (
    GitHubClient,
    Arc<Mutex<Vec<usize>>>,
    tokio::task::JoinHandle<()>,
) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let pages = Arc::new(Mutex::new(Vec::new()));
    let recorded = pages.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0);
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(request).unwrap();
            let target = request.split_whitespace().nth(1).unwrap();
            let url = reqwest::Url::parse(&format!("http://stub.invalid{target}")).unwrap();
            assert_eq!(url.path(), "/repos/acme/app/actions/workflows/42/runs");
            let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(query.get("per_page").map(String::as_str), Some("100"));
            let page: usize = query["page"].parse().unwrap();
            recorded.lock().await.push(page);
            let runs: Vec<_> = (1..=1025)
                .rev()
                .skip((page - 1) * 100)
                .take(100)
                .map(history_run)
                .collect();
            let body = serde_json::json!({"total_count": 1025, "workflow_runs": runs}).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let http_client = reqwest::Client::new();
    let octocrab = Octocrab::builder()
        .add_retry_config(octocrab::service::middleware::retry::RetryConfig::None)
        .personal_token("ghp_stub")
        .base_uri(&base_url)
        .unwrap()
        .build_with_reqwest(http_client.clone())
        .unwrap();
    let client = GitHubClient::new(octocrab, http_client, "ghp_stub".into(), &base_url).unwrap();
    (client, pages, server)
}

async fn assert_history_pages(limit: usize, expected: usize, last_page: usize) {
    let (client, pages, server) = history_fixture().await;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        client.fetch_run_history("acme", "app", 42, limit),
    )
    .await;
    server.abort();
    let runs = result.unwrap().unwrap();
    assert_eq!(runs.len(), expected);
    assert_eq!(
        runs.iter().map(|run| run.id.0).collect::<Vec<_>>(),
        (1..=1025u64).rev().take(expected).collect::<Vec<_>>()
    );
    assert_eq!(
        runs.iter().map(|run| run.run_number).collect::<Vec<_>>(),
        (1..=1025i64).rev().take(expected).collect::<Vec<_>>()
    );
    assert_eq!(*pages.lock().await, (1..=last_page).collect::<Vec<_>>());
}

#[tokio::test]
async fn history_fetches_page_eleven_and_stops_on_partial_page() {
    assert_history_pages(1200, 1025, 11).await;
}

#[tokio::test]
async fn history_preserves_limit_on_page_eleven() {
    assert_history_pages(1007, 1007, 11).await;
}

#[tokio::test]
async fn history_zero_limit_sends_no_requests() {
    assert_history_pages(0, 0, 0).await;
}

#[tokio::test]
async fn older_run_details_search_past_page_ten() {
    let (client, pages, server) = history_fixture().await;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        client.fetch_run_by_number("acme", "app", 42, 1),
    )
    .await;
    server.abort();

    let run = result.unwrap().unwrap();
    assert_eq!(run.id.0, 1);
    assert_eq!(run.run_number, 1);
    assert_eq!(*pages.lock().await, (1..=11).collect::<Vec<_>>());
}

#[tokio::test]
async fn missing_run_details_stop_after_the_partial_final_page() {
    let (client, pages, server) = history_fixture().await;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        client.fetch_run_by_number("acme", "app", 42, 0),
    )
    .await;
    server.abort();

    assert!(matches!(
        result.unwrap(),
        Err(PluginError::PipelineNotFound(_))
    ));
    assert_eq!(*pages.lock().await, (1..=11).collect::<Vec<_>>());
}
