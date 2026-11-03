use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};

use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::{
    TcpListener,
    TcpStream,
};

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
}

type Handler = Arc<dyn Fn(&Request) -> u16 + Send + Sync>;

pub struct Stub {
    pub url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Stub {
    pub async fn start(handler: impl Fn(&Request) -> u16 + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let recorded = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(serve(stream, Arc::clone(&handler), Arc::clone(&recorded)));
            }
        });
        Self { url, requests }
    }

    pub fn count(&self, method: &str, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|r| r.method == method && r.path == path)
            .count()
    }
}

async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(position) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();
    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    while buffer.len() < head_end + length {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    Some(Request { method, path })
}

async fn serve(mut stream: TcpStream, handler: Handler, recorded: Arc<Mutex<Vec<Request>>>) {
    let Some(request) = read_request(&mut stream).await else {
        return;
    };
    let status = handler(&request);
    recorded
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(request);
    let response =
        format!("HTTP/1.1 {status} Stub\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}
