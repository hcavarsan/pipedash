use std::io::{
    Read,
    Write,
};
use std::net::TcpListener;

use super::*;

async fn serve(raw: &'static str) -> reqwest::Response {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(raw.as_bytes());
        }
    });
    reqwest::Client::new()
        .get(format!("http://{addr}/log"))
        .send()
        .await
        .unwrap()
}

async fn drain(mut rx: mpsc::Receiver<Bytes>) -> Vec<u8> {
    let mut got = Vec::new();
    while let Some(chunk) = rx.recv().await {
        got.extend_from_slice(&chunk);
    }
    got
}

async fn ranged(raw: &'static str, offset: u64) -> (PluginResult<RangedRead>, Vec<u8>, bool, u64) {
    let response = serve(raw).await;
    let (tx, rx) = mpsc::channel(16);
    let mut sink = LogSink::new(tx);
    let outcome = sink.pipe_range(response, offset).await;
    let restarted = sink.restarted();
    let written = sink.bytes_written();
    drop(sink);
    (outcome, drain(rx).await, restarted, written)
}

#[tokio::test]
async fn pipe_delivers_chunked_bytes_exactly() {
    let response = serve(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
         5\r\nhello\r\n7\r\n world\n\r\n3\r\nend\r\n0\r\n\r\n",
    )
    .await;
    let (tx, rx) = mpsc::channel(16);
    let mut sink = LogSink::new(tx);
    sink.pipe(response).await.unwrap();
    assert_eq!(sink.bytes_written(), 15);
    drop(sink);
    assert_eq!(drain(rx).await, b"hello world\nend");
}

#[tokio::test]
async fn pipe_after_drops_the_overlap_across_chunks() {
    let response = serve(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
         2\r\nab\r\n3\r\ncde\r\n1\r\nf\r\n0\r\n\r\n",
    )
    .await;
    let (tx, rx) = mpsc::channel(16);
    let mut sink = LogSink::new(tx);
    sink.pipe_after(response, 4).await.unwrap();
    assert_eq!(sink.bytes_written(), 2);
    drop(sink);
    assert_eq!(drain(rx).await, b"ef");
}

#[test]
fn content_hash_is_fnv1a_64_and_continues_across_chunks() {
    assert_eq!(content_hash(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(content_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(content_hash(b"foobar"), 0x8594_4171_f739_67e8);
    assert_eq!(
        extend_content_hash(content_hash(b"foo"), b"bar"),
        content_hash(b"foobar")
    );
}

#[test]
fn content_range_reads_spans_totals_and_unknowns() {
    let parse = |value: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_RANGE, value.parse().unwrap());
        ContentRange::parse(&headers)
    };
    assert_eq!(
        parse("bytes 5-9/10"),
        Some(ContentRange {
            span: Some((5, 9)),
            total: Some(10)
        })
    );
    assert_eq!(
        parse("bytes 5-9/*"),
        Some(ContentRange {
            span: Some((5, 9)),
            total: None
        })
    );
    assert_eq!(
        parse("bytes */10"),
        Some(ContentRange {
            span: None,
            total: Some(10)
        })
    );
    for malformed in ["items 5-9/10", "bytes 5-9", "bytes a-9/10", "bytes 5-/10"] {
        assert_eq!(parse(malformed), None, "{malformed}");
    }
    assert_eq!(ContentRange::parse(&HeaderMap::new()), None);
}

#[tokio::test]
async fn range_reply_starting_at_the_offset_appends() {
    let (outcome, got, restarted, _) = ranged(
        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-9/10\r\nContent-Length: 5\r\n\r\nworld",
        5,
    )
    .await;
    assert_eq!(outcome.unwrap(), RangedRead::Appended(10));
    assert_eq!(got, b"world");
    assert!(!restarted);
}

#[tokio::test]
async fn range_reply_starting_elsewhere_or_unmarked_is_refetched_unwritten() {
    for raw in [
        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 6-9/10\r\nContent-Length: 4\r\n\r\norld",
        "HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\n\r\nworld",
        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */3\r\nContent-Length: 0\r\n\r\n",
        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n\r\n",
        "HTTP/1.1 204 No Content\r\n\r\n",
    ] {
        let (outcome, got, restarted, written) = ranged(raw, 5).await;
        assert_eq!(outcome.unwrap(), RangedRead::Refetch, "{raw}");
        assert!(got.is_empty() && !restarted && written == 0, "{raw}");
    }
}

#[tokio::test]
async fn range_not_satisfiable_at_the_exact_total_appends_nothing() {
    let (outcome, got, restarted, _) = ranged(
        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */5\r\nContent-Length: 0\r\n\r\n",
        5,
    )
    .await;
    assert_eq!(outcome.unwrap(), RangedRead::Appended(5));
    assert!(got.is_empty() && !restarted);
}

#[tokio::test]
async fn full_body_after_a_range_restarts_before_writing() {
    let (outcome, got, restarted, _) =
        ranged("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nhelloworld", 5).await;
    assert_eq!(outcome.unwrap(), RangedRead::Replaced(10));
    assert_eq!(got, b"helloworld");
    assert!(restarted);
}

#[tokio::test]
async fn range_reply_with_a_server_error_fails() {
    let (outcome, got, restarted, _) = ranged(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
        5,
    )
    .await;
    assert!(matches!(
        outcome,
        Err(PluginError::ServiceUnavailable { .. })
    ));
    assert!(got.is_empty() && !restarted);
}

#[tokio::test]
async fn write_after_reader_dropped_is_internal() {
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    let mut sink = LogSink::new(tx);
    let err = sink.write(Bytes::from_static(b"x")).await.unwrap_err();
    assert!(matches!(err, PluginError::Internal(_)));
}

#[test]
fn step_nodes_hang_off_their_job() {
    let node = LogNode::step(
        "7",
        2,
        "build".to_string(),
        PipelineStatus::Running,
        None,
        None,
    );
    assert_eq!(node.id, "7:2");
    assert_eq!(node.parent_id.as_deref(), Some("7"));
    assert_eq!(node.kind, LogNodeKind::Step);
    assert_eq!((node.log, node.web_url), (None, None));
}
