use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::{
    TcpListener,
    TcpStream,
};

pub(crate) struct Reply {
    pub status: &'static str,
    pub headers: &'static str,
    pub body: &'static str,
}

async fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut data = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..end]).into_owned();
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if data.len() >= end + 4 + length {
                return Some(head);
            }
        }
        let n = stream.read(&mut buffer).await.ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&buffer[..n]);
    }
}

pub(crate) async fn stub_server(
    requests: Arc<AtomicUsize>, delay: Duration, reply: fn(&str) -> Reply,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let requests = requests.clone();
            tokio::spawn(async move {
                let Some(head) = read_request(&mut stream).await else {
                    return;
                };
                requests.fetch_add(1, Ordering::SeqCst);
                let path = head
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let reply = reply(path);
                tokio::time::sleep(delay).await;
                let response = format!(
                    "HTTP/1.1 {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.status,
                    reply.headers,
                    reply.body.len(),
                    reply.body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}
